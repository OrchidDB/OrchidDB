//! Native SQL updates for explicitly writable mapped queries.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};

use super::{MappedGraphEngine, children};
use crate::ir::catalog::PropertyGraph;
use crate::ir::expr::IrExpr;
use crate::ir::plan::{
    GraphPlan, LabelExpr, Node, ProjectErrorPolicy, ProjectMode, ProjectionItem, SetMode,
};
use crate::ir::rel::mapping::MappedSource;
use crate::ir::rel::sql::{SqlDialect, SqlExecutor, SqlValue, prepare_with_external};
use crate::ir::value::Value;
use crate::language::cypher::{
    ast::Clause, parameters::bind_parameters, parser::parse_query, planner::CypherPlanner,
};

impl MappedGraphEngine {
    /// Execute one `MATCH ... SET binding.property = expression` assignment
    /// directly as DuckDB UPDATE, returning the number of affected rows.
    ///
    /// This explicit write API accepts no RETURN, CREATE, DELETE, MERGE, map
    /// replacement, or multiple assignments. The target must resolve to one
    /// mapped table with unique, non-null matching identities. Query mappings
    /// and identifier/endpoint changes are rejected. Expressions and predicates
    /// must lower completely to SQL; there is no graph-runtime fallback.
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
        let plan = CypherPlanner::new()
            .plan(&parsed)
            .map_err(|e| e.to_string())?;
        let Node::GraphReturn {
            input: write,
            result_form,
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
        let (source, id_column, property) = if *is_edge {
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
        let MappedSource::Table(table) = source else {
            return Err("SQL updates require a table-backed mapping, not a query mapping".into());
        };
        let quote = |name: &str| SqlDialect::DuckDb.quote_ident(name);
        let table = datafusion::common::TableReference::from(table.as_str())
            .to_vec()
            .iter()
            .map(|part| quote(part))
            .collect::<Vec<_>>()
            .join(".");
        let id_column = quote(id_column);
        let property = quote(property);
        let read = GraphPlan {
            policy: plan.policy.clone(),
            root: Box::new(Node::GraphReturn {
                fields: vec!["__cg_update_id".into(), "__cg_update_value".into()],
                result_form: *result_form,
                input: Box::new(Node::GraphProject {
                    mode: ProjectMode::ReplaceScope,
                    error_policy: ProjectErrorPolicy::PropagateError,
                    items: vec![
                        ProjectionItem {
                            alias: "__cg_update_id".into(),
                            expr: IrExpr::Binding(crate::ir::rel::id_col(binding)),
                        },
                        ProjectionItem {
                            alias: "__cg_update_value".into(),
                            expr: item.value.clone(),
                        },
                    ],
                    input: input.clone(),
                }),
            }),
        };
        let lowered = self
            .backend()
            .lower(&read, &PropertyGraph::new())
            .map_err(|e| e.to_string())?;
        let prepared = prepare_with_external(
            &lowered,
            SqlDialect::DuckDb,
            &self.mapping.physical_table_names(),
        )
        .await
        .map_err(|e| e.to_string())?;
        static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);
        let temporary = quote(&format!(
            "__crabgraph_update_{}",
            NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
        ));
        let automatic = !self.executor.in_transaction();
        if automatic {
            self.executor.begin().map_err(|e| e.to_string())?;
        }
        let mut created = false;
        let result = (|| -> Result<usize, String> {
            self.executor
                .run(
                    &prepared.setup,
                    &format!("CREATE TEMP TABLE {temporary} AS {}", prepared.query),
                )
                .map_err(|e| e.to_string())?;
            created = true;
            // Duplicate matches cannot be reduced to an arbitrary UPDATE FROM
            // row: repeated Cypher updates can depend on the previous value.
            let duplicates = self.executor.run(&[], &format!(
                "SELECT count(*) FROM (SELECT \"__cg_update_id\" FROM {temporary} GROUP BY \"__cg_update_id\" HAVING count(*) > 1 OR \"__cg_update_id\" IS NULL)"
            )).map_err(|e| e.to_string())?;
            if integer_result(&duplicates)? != 0 {
                return Err("SQL update matched duplicate or null graph identifiers".into());
            }
            let ambiguous = self.executor.run(&[], &format!(
                "SELECT count(*) FROM (SELECT t.\"__cg_update_id\" FROM {temporary} t LEFT JOIN {table} s ON s.{id_column} = t.\"__cg_update_id\" GROUP BY t.\"__cg_update_id\" HAVING count(s.{id_column}) <> 1)"
            )).map_err(|e| e.to_string())?;
            if integer_result(&ambiguous)? != 0 {
                return Err("mapped source does not have unique graph identifiers".into());
            }
            let result = self.executor.run(&[], &format!(
                "UPDATE {table} AS source SET {property} = candidates.\"__cg_update_value\" FROM {temporary} AS candidates WHERE source.{id_column} = candidates.\"__cg_update_id\""
            )).map_err(|e| e.to_string())?;
            integer_result(&result)
        })();
        let cleanup = if created {
            self.executor
                .execute_batch(&format!("DROP TABLE {temporary}"))
                .map_err(|e| e.to_string())
        } else {
            Ok(())
        };
        let result = result.and_then(|count| cleanup.map(|()| count));
        if automatic {
            match result {
                Ok(count) => {
                    if let Err(error) = self.executor.commit() {
                        let _ = self.executor.rollback();
                        return Err(error.to_string());
                    }
                    Ok(count)
                }
                Err(error) => {
                    let _ = self.executor.rollback();
                    Err(error)
                }
            }
        } else {
            result
        }
    }
}

fn integer_result(rows: &[Vec<SqlValue>]) -> Result<usize, String> {
    match rows {
        [row] => match row.as_slice() {
            [SqlValue::Int(value)] => usize::try_from(*value).map_err(|e| e.to_string()),
            _ => Err("DuckDB did not return an affected-row count".into()),
        },
        _ => Err("DuckDB did not return an affected-row count".into()),
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
