//! Re-enter SQL above a genuine runtime boundary using typed batch inputs.
use super::*;
use datafusion::logical_expr::{LogicalPlanBuilder, Projection};

pub(super) fn cut_inputs(
    plan: &LogicalPlan,
    eligibility: &mut SqlEligibility,
    next: &mut usize,
    external: &std::collections::BTreeSet<String>,
) -> Result<Option<(LogicalPlan, Vec<(LogicalPlan, sql::TableData)>)>> {
    if matches!(plan, LogicalPlan::Extension(_) | LogicalPlan::TableScan(_))
        || plan.inputs().is_empty()
    {
        return Ok(None);
    }
    let mut inputs = Vec::new();
    let mut bindings = Vec::new();
    for input in plan.inputs() {
        if eligibility.visit(input).is_none() {
            inputs.push(input.clone());
            continue;
        }
        if let Some((lowered, nested)) = cut_inputs(input, eligibility, next, external)? {
            inputs.push(lowered);
            bindings.extend(nested);
            continue;
        }
        // Do not erase the ordering contract of a sorted input by transporting
        // it through an unordered SQL table. An explicit sort above the cut is safe.
        let mut ordered = false;
        input.apply(|node| {
            // A boundary owns its internal ordering semantics. A ranking
            // window or limit inside a callback does not impose an ordering
            // contract on the boundary's exported relation.
            if matches!(node, LogicalPlan::Extension(_)) {
                return Ok(TreeNodeRecursion::Jump);
            }
            ordered |= matches!(
                node,
                LogicalPlan::Sort(_) | LogicalPlan::Limit(_) | LogicalPlan::Window(_)
            );
            Ok(TreeNodeRecursion::Continue)
        })?;
        if ordered {
            return Ok(None);
        }
        let name = loop {
            let name = format!("__orchid_runtime_input_{}", *next);
            *next += 1;
            if !external.contains(&name) {
                break name;
            }
        };
        let schema = if input.schema().fields().is_empty() {
            Arc::new(arrow::datatypes::Schema::new(vec![arrow::datatypes::Field::new(
                "__orchid_cardinality", arrow::datatypes::DataType::Int64, false)]))
        } else { Arc::new(input.schema().as_arrow().clone()) };
        // Duplicate SQL field names cannot form a table interface.
        let names = schema
            .fields()
            .iter()
            .map(|f| f.name())
            .collect::<std::collections::BTreeSet<_>>();
        if names.len() != schema.fields().len() {
            return Ok(None);
        }
        let provider = datafusion::datasource::MemTable::try_new(schema.clone(), vec![vec![]])?;
        let scan = LogicalPlanBuilder::scan(
            name.clone(),
            datafusion::datasource::provider_as_source(Arc::new(provider)),
            None,
        )?
        .build()?;
        let exprs = scan
            .schema()
            .columns()
            .into_iter()
            .zip(input.schema().iter())
            .map(|(column, (qualifier, field))| {
                Expr::Column(column).alias_qualified(qualifier.cloned(), field.name())
            })
            .collect();
        inputs.push(LogicalPlan::Projection(Projection::try_new_with_schema(
            exprs,
            Arc::new(scan),
            input.schema().clone(),
        )?));
        bindings.push((
            input.clone(),
            sql::TableData {
                name,
                schema,
                batches: vec![],
            },
        ));
    }
    if bindings.is_empty() {
        return Ok(None);
    }
    let candidate = if let LogicalPlan::Unnest(unnest) = plan {
        let mut unnest = unnest.clone();
        unnest.input = Arc::new(inputs.remove(0));
        LogicalPlan::Unnest(unnest)
    } else {
        plan.with_new_exprs(plan.expressions(), inputs)?
    };
    let mut check = SqlEligibility {
        dialect: eligibility.dialect,
        language_functions: eligibility.language_functions,
        ..Default::default()
    };
    Ok(check
        .visit(&candidate)
        .is_none()
        .then_some((candidate, bindings)))
}


/// Preserve logical qualifiers while giving caller-session Arrow inputs plain,
/// collision-free names. Keep source SQL identifiers separate from those names.
pub(super) fn bind_sources(
    plan: LogicalPlan,
    external: &std::collections::BTreeSet<String>,
    reserved: &mut std::collections::BTreeSet<String>,
    transport: bool,
) -> Result<(LogicalPlan, Vec<SourceTable>)> {
    use datafusion::common::{TableReference, tree_node::Transformed};
    use datafusion::logical_expr::TableScan;
    let mut occupied = reserved.clone();
    plan.apply_with_subqueries(|node| {
        if let LogicalPlan::TableScan(scan) = node {
            occupied.insert(scan.table_name.to_string());
        }
        Ok(TreeNodeRecursion::Continue)
    })?;
    let mut sources = std::collections::BTreeMap::<TableReference, SourceTable>::new();
    let mut next = 0;
    let plan = plan
        .transform_up_with_subqueries(|node| {
            let LogicalPlan::TableScan(scan) = &node else {
                return Ok(Transformed::no(node));
            };
            if !external.contains(&scan.table_name.to_string())
                && !external.contains(scan.table_name.table())
            {
                return Ok(Transformed::no(node));
            }
            let source = sources.entry(scan.table_name.clone()).or_insert_with(|| {
                let input_name = if transport {
                    loop {
                        let name = format!("__orchid_source_input_{next}");
                        next += 1;
                        if occupied.insert(name.clone()) {
                            reserved.insert(name.clone());
                            break name;
                        }
                    }
                } else {
                    scan.table_name.to_string()
                };
                SourceTable {
                    name: scan.table_name.to_quoted_string(),
                    input_name,
                    schema: scan.source.schema(),
                    uses: 0,
                }
            });
            source.uses += 1;
            if !transport {
                return Ok(Transformed::no(node));
            }
            let name = TableReference::bare(source.input_name.clone());
            let filters = scan
                .filters
                .iter()
                .cloned()
                .map(|expr| {
                    Ok(expr
                        .transform_up(|expr| {
                            if let Expr::Column(mut column) = expr {
                                if column
                                    .relation
                                    .as_ref()
                                    .is_none_or(|r| r == &scan.table_name)
                                {
                                    column.relation = Some(name.clone());
                                }
                                return Ok(Transformed::yes(Expr::Column(column)));
                            }
                            Ok(Transformed::no(expr))
                        })?
                        .data)
                })
                .collect::<Result<Vec<_>>>()?;
            let replacement = LogicalPlan::TableScan(TableScan::try_new(
                name,
                scan.source.clone(),
                scan.projection.clone(),
                filters,
                scan.fetch,
            )?);
            let expressions = replacement
                .schema()
                .columns()
                .into_iter()
                .zip(scan.projected_schema.iter())
                .map(|(column, (qualifier, field))| {
                    Expr::Column(column).alias_qualified(qualifier.cloned(), field.name())
                })
                .collect();
            Ok(Transformed::yes(LogicalPlan::Projection(
                Projection::try_new_with_schema(
                    expressions,
                    Arc::new(replacement),
                    scan.projected_schema.clone(),
                )?,
            )))
        })?
        .data;
    Ok((plan, sources.into_values().collect()))
}
