//! Affected-row compatibility API over the common mutation executor.

use std::collections::BTreeMap;

use super::{MappedGraphEngine, children};
use crate::ir::expr::IrExpr;
use crate::ir::plan::{LabelExpr, Node, SetMode};
use crate::ir::rel::mapping::MappedSource;

use crate::ir::value::Value;
use crate::language::cypher::{
    ast::Clause, parameters::bind_parameters, parser::parse_query, planner::CypherPlanner,
};

impl MappedGraphEngine {
    /// Execute one `MATCH ... SET binding.property = expression` assignment
    /// through the shared executor, returning the number of affected rows.
    ///
    /// This explicit write API accepts no RETURN, CREATE, DELETE, MERGE, map
    /// replacement, or multiple assignments. The target must resolve to one
    /// mapped table with unique, non-null matching identities. Query mappings
    /// and identifier/endpoint changes are rejected. Expressions and predicates
    /// use the same graph runtime as ordinary queries.
    ///
    /// Joins an explicit executor transaction, otherwise commits atomically.
    /// After a DuckDB transaction error, roll back the explicit transaction.
    pub async fn cypher_update(&mut self, query: &str) -> Result<usize, String> {
        self.cypher_update_with_params(query, &BTreeMap::new())
            .await
    }

    pub async fn cypher_update_with_params(
        &mut self,
        query: &str,
        parameters: &BTreeMap<String, Value>,
    ) -> Result<usize, String> {
        let mut parsed = parse_query(query).map_err(|e| e.to_string())?;
        bind_parameters(&mut parsed, parameters)?;
        if !parsed.unions.is_empty()
            || parsed
                .clauses
                .iter()
                .any(|clause| !matches!(clause, Clause::Match(_) | Clause::Set(_)))
            || parsed
                .clauses
                .iter()
                .filter(|clause| matches!(clause, Clause::Set(_)))
                .count()
                != 1
            || !matches!(parsed.clauses.last(), Some(Clause::Set(_)))
        {
            return Err(
                "SQL updates require MATCH followed by one SET property assignment, without RETURN"
                    .into(),
            );
        }
        let plan = self.with_functions(|| {
            CypherPlanner::new()
                .plan(&parsed)
                .map_err(|e| e.to_string())
        })?;
        let Node::GraphReturn {
            input: write,
            result_form: _,
            ..
        } = plan.root.as_ref()
        else {
            return Err("unsupported SQL update plan".into());
        };
        let Node::GraphSetProperty { items, input } = write.as_ref() else {
            return Err("unsupported SQL update plan".into());
        };
        let [item] = items.as_slice() else {
            return Err("SQL updates currently support exactly one assignment".into());
        };
        if item.mode != SetMode::Property {
            return Err("SQL updates require a single named property assignment".into());
        }
        let IrExpr::Binding(binding) = &item.target else {
            return Err("SQL update target must be a bound graph element".into());
        };
        let mut targets = Vec::new();
        find_targets(input, binding, &mut targets);
        let [(is_edge, label)] = targets.as_slice() else {
            return Err(
                "SQL update target must have one statically known label or relationship type"
                    .into(),
            );
        };
        let (source, _id_column, _property) = if *is_edge {
            let mapped = self.mapping.edge(label).ok_or("missing edge mapping")?;
            let id = mapped
                .id_column
                .as_ref()
                .ok_or("SQL edge updates require an explicit edge ID column")?;
            let property = mapped
                .properties
                .get(&item.key)
                .ok_or("unmapped update property")?;
            if property == id || property == &mapped.src_column || property == &mapped.dst_column {
                return Err("SQL updates cannot change graph identifiers or edge endpoints".into());
            }
            (&mapped.source, id, property)
        } else {
            let mapped = self.mapping.node(label).ok_or("missing node mapping")?;
            let property = mapped
                .properties
                .get(&item.key)
                .ok_or("unmapped update property")?;
            if property == &mapped.id_column {
                return Err("SQL updates cannot change graph identifiers".into());
            }
            (&mapped.source, &mapped.id_column, property)
        };
        let MappedSource::Table(_table) = source else {
            return Err("SQL updates require a table-backed mapping, not a query mapping".into());
        };
        let binding = binding.replace('`', "``");
        let mut check = parsed.clone();
        check.clauses.pop();
        check.clauses.extend(
            parse_query(&format!(
                "RETURN count(*) AS __rows, count(DISTINCT `{binding}`) AS __unique"
            ))
            .map_err(|e| e.to_string())?
            .clauses,
        );
        let check =
            self.with_functions(|| CypherPlanner::new().plan(&check).map_err(|e| e.to_string()))?;
        let query = format!(
            "{} RETURN count(DISTINCT `{binding}`) AS __updated",
            query.trim().trim_end_matches(';')
        );
        let automatic = !self.executor.in_transaction();
        if automatic {
            self.executor.begin().map_err(|e| e.to_string())?;
        }
        let result = async {
            let counts = self.run_plan(&check).await?;
            let count = |index| -> Result<i64, String> {
                Ok(counts
                    .batch
                    .column(index)
                    .as_any()
                    .downcast_ref::<arrow::array::Int64Array>()
                    .ok_or("invalid match count")?
                    .value(0))
            };
            if count(0)? != count(1)? {
                return Err("SQL updates require unique matching identities".into());
            }
            let result = self.cypher_with_params(&query, parameters).await?;
            let count = result
                .batch
                .column(0)
                .as_any()
                .downcast_ref::<arrow::array::Int64Array>()
                .ok_or("runtime did not return an affected-row count")?;
            if count.len() != 1 {
                return Err("invalid affected-row count".into());
            }
            usize::try_from(count.value(0)).map_err(|e| e.to_string())
        }
        .await;
        if automatic {
            if result.is_ok() {
                self.executor.commit().map_err(|e| e.to_string())?;
            } else {
                let _ = self.executor.rollback();
            }
        }
        result
    }
}

fn find_targets(node: &Node, binding: &str, output: &mut Vec<(bool, String)>) {
    let target = match node {
        Node::GraphNodeScan {
            binding: name,
            labels,
            ..
        } if name == binding => Some((false, labels)),
        Node::GraphRelScan {
            binding: name,
            types,
            ..
        } if name == binding => Some((true, types)),
        Node::GraphExpand {
            target,
            target_labels,
            ..
        } if target == binding => Some((false, target_labels)),
        Node::GraphExpand {
            rel_binding: Some(name),
            rel_types,
            ..
        } if name == binding => Some((true, rel_types)),
        _ => None,
    };
    if let Some((is_edge, LabelExpr::AnyOf(labels) | LabelExpr::AllOf(labels))) = target {
        if let [label] = labels.as_slice() {
            output.push((is_edge, label.clone()));
        }
    }
    for child in children(node) {
        find_targets(child, binding, output);
    }
}
