//! Re-enter SQL above a genuine runtime boundary using typed batch inputs.
use super::*;
use datafusion::logical_expr::{LogicalPlanBuilder, Projection};

pub(super) fn cut_inputs(
    plan: &LogicalPlan,
    eligibility: &mut SqlEligibility,
    next: &mut usize,
    external: &std::collections::BTreeSet<String>,
) -> Result<Option<(LogicalPlan, Vec<(LogicalPlan, sql::TableData)>)>> {
    if matches!(
        plan,
        LogicalPlan::Extension(_) | LogicalPlan::TableScan(_) | LogicalPlan::Unnest(_)
    ) || plan.inputs().is_empty()
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
        // A zero-column relation needs a separate row-count representation.
        if input.schema().fields().is_empty() {
            return Ok(None);
        }
        // Do not erase the ordering contract of a sorted input by transporting
        // it through an unordered SQL table. An explicit sort above the cut is safe.
        let mut ordered = false;
        input.apply(|node| {
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
        let schema = Arc::new(input.schema().as_arrow().clone());
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
    let candidate = plan.with_new_exprs(plan.expressions(), inputs)?;
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
