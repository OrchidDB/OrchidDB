//! PostgreSQL implementation of an ordered first row per partition.
//! Match the exact generated slice; other windows, offsets and predicates stay put.
use super::*;
use datafusion::logical_expr::LogicalPlanBuilder;

// DISTINCT ON consumes the selection ordering. An unbounded input Sort is
// redundant here; a fetched sort is an independent slice and must survive.
fn consume_order(plan: LogicalPlan) -> LogicalPlan {
    match plan {
        LogicalPlan::Sort(sort) if sort.fetch.is_none() => sort.input.as_ref().clone(),
        LogicalPlan::Filter(mut filter) => {
            filter.input = Arc::new(consume_order(filter.input.as_ref().clone()));
            LogicalPlan::Filter(filter)
        }
        other => other,
    }
}

pub(super) fn rewrite(plan: LogicalPlan) -> SqlResult<LogicalPlan> {
    Ok(plan.transform_up_with_subqueries(|node| {
        let LogicalPlan::Filter(filter) = &node else { return Ok(Transformed::no(node)); };
        let LogicalPlan::Window(window) = filter.input.as_ref() else { return Ok(Transformed::no(node)); };
        let [Expr::Alias(alias)] = window.window_expr.as_slice() else { return Ok(Transformed::no(node)); };
        let Expr::WindowFunction(function) = alias.expr.as_ref() else { return Ok(Transformed::no(node)); };
        if !matches!(&function.fun, datafusion::logical_expr::WindowFunctionDefinition::WindowUDF(f)
            if f.inner().as_any().is::<datafusion::functions_window::row_number::RowNumber>()) || function.params.partition_by.is_empty()
            || function.params.order_by.is_empty() {
            return Ok(Transformed::no(node));
        }
        let rank = Expr::Column(datafusion::common::Column::new_unqualified(&alias.name));
        let expected = rank.clone().gt(lit(0_u64)).and(rank.lt_eq(lit(1_u64)));
        if filter.predicate != expected { return Ok(Transformed::no(node)); }
        // DISTINCT ON and ranking must evaluate identical deterministic keys.
        if !super::super::list_comparison::immutable_source_plan(&node) {
            return Ok(Transformed::no(node));
        }
        let keys = function.params.partition_by.clone();
        let mut order = keys.iter().cloned().map(|e| e.sort(true, false)).collect::<Vec<_>>();
        order.extend(function.params.order_by.clone());
        let columns = window.input.schema().columns().into_iter().map(Expr::Column).collect::<Vec<_>>();
        let selected = LogicalPlanBuilder::from(consume_order(window.input.as_ref().clone()))
            .distinct_on(keys, columns, Some(order))?.build()?;
        let mut output = selected.schema().columns().into_iter().map(Expr::Column).collect::<Vec<_>>();
        output.push(lit(1_u64).alias_qualified(alias.relation.clone(), &alias.name));
        let result = LogicalPlanBuilder::from(selected).project(output)?.build()?;
        Ok(Transformed::yes(result))
    })?.data)
}
