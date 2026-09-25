//! Semantic rules shared by lowering and SQL IR optimization. Rules consume
//! proven properties; they never infer uniqueness from column naming alone.
use super::*;
use datafusion::common::Dependency;

/// A non-null unique determinant contained in `keys` proves at most one row
/// per key, even if the relation carries additional payload columns.
pub(super) fn unique_on(plan: &LogicalPlan, keys: &[Expr]) -> bool {
    let schema = plan.schema();
    let indices = keys
        .iter()
        .filter_map(|expr| match expr {
            Expr::Column(column) => schema.index_of_column(column).ok(),
            _ => None,
        })
        .collect::<BTreeSet<_>>();
    schema.functional_dependencies().iter().any(|dep| {
        dep.mode == Dependency::Single
            && dep.target_indices.len() == schema.fields().len()
            && dep
                .source_indices
                .iter()
                .all(|index| indices.contains(index))
            && (!dep.nullable
                || dep
                    .source_indices
                    .iter()
                    .all(|index| !schema.field(*index).is_nullable()))
    })
}

/// A semi/anti join observes membership, never the number of equal right rows.
/// Only remove full-row DISTINCT: DISTINCT ON can select a different payload.
/// This rule retains projections, filters, LIMIT, and all expression evaluation.
pub(super) fn simplify_existence(plan: LogicalPlan) -> datafusion::common::Result<LogicalPlan> {
    // Dispatch this rule when a membership join is constructed. Queries
    // without a membership join pay no extra optimizer traversal.
    let LogicalPlan::Join(mut join) = plan else {
        return Ok(plan);
    };
    let input = match join.join_type {
        JoinType::LeftSemi | JoinType::LeftAnti => &mut join.right,
        JoinType::RightSemi | JoinType::RightAnti => &mut join.left,
        _ => return Ok(LogicalPlan::Join(join)),
    };
    if let LogicalPlan::Distinct(datafusion::logical_expr::Distinct::All(distinct)) = input.as_ref()
    {
        *input = distinct.clone();
    }
    Ok(LogicalPlan::Join(join))
}

/// Uniqueness makes an order-preserving dedup an identity operation. Keep the
/// original input (and hence its ordering and expression/error evaluation).
pub(super) fn eliminate_redundant_distinct(
    plan: &LogicalPlan,
    keys: &[Expr],
) -> Option<LogicalPlan> {
    unique_on(plan, keys).then(|| plan.clone())
}

/// A scalar singleton expansion retains parent multiplicity. New binding only:
/// shadowing graph bindings may require removing several identity columns.
pub(super) fn fold_singleton_unwind(
    plan: &LogicalPlan,
    bind: &str,
    values: &[Value],
) -> RelResult<Option<LogicalPlan>> {
    if values.len() != 1 || has_exact_col(plan, bind) || has_binding_shape(plan, bind).is_some() {
        return Ok(None);
    }
    // Other values may require the language-specific tagged representation.
    if !matches!(
        &values[0],
        Value::Null | Value::Bool(_) | Value::Int(_) | Value::Float(_) | Value::String(_)
    ) {
        return Ok(None);
    }
    let mut columns = existing_columns(plan, &BTreeSet::new());
    columns.push(value_literal_expr(&values[0])?.alias(bind));
    Ok(Some(
        LogicalPlanBuilder::from(plan.clone())
            .project(columns)?
            .build()?,
    ))
}

impl LoweringContext<'_> {
    /// Specialize semantic distinct before it expands into representative
    /// selection windows. Only a count of rows may discard the payload.
    pub(super) fn lower_cardinality_input(&mut self, node: &Node) -> RelResult<LoweredNode> {
        let Node::GraphDistinct { keys, input, .. } = node else {
            return self.lower_node(node);
        };
        let lowered = self.lower_node(input)?;
        // RDF equality additionally includes term kind, language, and datatype.
        if lowered
            .plan
            .schema()
            .fields()
            .iter()
            .any(|f| f.name().starts_with("__rdf:term:"))
        {
            return self.lower_node(node);
        }
        let mut keys = distinct_partition(&lowered.plan, keys);
        let mut unique = Vec::new();
        for key in keys.drain(..) {
            if !unique.contains(&key) {
                unique.push(key);
            }
        }
        if unique_on(&lowered.plan, &unique) {
            return Ok(lowered);
        }
        let plan = LogicalPlanBuilder::from(lowered.plan.clone())
            .project(unique)?
            .distinct()?
            .build()?;
        Ok(lowered.with_plan(plan))
    }

    /// Existence ignores duplicate representatives and their bulk. Retain
    /// the complete child computation, including filters and expressions.
    pub(super) fn lower_existence_input(&mut self, mut node: &Node) -> RelResult<LoweredNode> {
        while let Node::GraphDistinct { input, .. } = node {
            node = input;
        }
        self.lower_node(node)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use datafusion::common::{Constraint, Constraints};

    fn table(name: &str, values: Vec<Option<i64>>, primary: bool) -> LogicalPlan {
        let nullable = values.iter().any(Option::is_none);
        let schema = Arc::new(Schema::new(vec![Field::new(
            "x",
            DataType::Int64,
            nullable,
        )]));
        let batch =
            RecordBatch::try_new(schema.clone(), vec![Arc::new(Int64Array::from(values))]).unwrap();
        let constraints = Constraints::new_unverified(vec![if primary {
            Constraint::PrimaryKey(vec![0])
        } else {
            Constraint::Unique(vec![0])
        }]);
        let provider = MemTable::try_new(schema, vec![vec![batch]])
            .unwrap()
            .with_constraints(constraints);
        LogicalPlanBuilder::scan(name, provider_as_source(Arc::new(provider)), None)
            .unwrap()
            .build()
            .unwrap()
    }

    #[test]
    fn nullable_unique_does_not_prove_scalar_cardinality() {
        let nullable = table("nullable", vec![None, None], false);
        assert!(!unique_on(&nullable, &[col_exact("x")]));
        let guarded =
            apply::guard_scalar_cardinality(LoweredNode::new(nullable), &["x".into()]).unwrap();
        assert!(
            guarded
                .plan
                .display_indent()
                .to_string()
                .contains("WindowAggr")
        );
        let keyed = table("keyed", vec![Some(1), Some(2)], true);
        assert!(unique_on(&keyed, &[col_exact("x")]));
        let original = keyed.clone();
        let guarded =
            apply::guard_scalar_cardinality(LoweredNode::new(keyed), &["x".into()]).unwrap();
        assert_eq!(guarded.plan, original);
        assert!(!unique_on(&original, &[]));
    }

    #[tokio::test]
    async fn semi_anti_rule_preserves_left_duplicates() {
        for (kind, expected) in [(JoinType::LeftSemi, 2), (JoinType::LeftAnti, 1)] {
            let left = LogicalPlanBuilder::values(vec![
                vec![lit(1_i64)],
                vec![lit(1_i64)],
                vec![lit(2_i64)],
            ])
            .unwrap()
            .alias("l")
            .unwrap()
            .build()
            .unwrap();
            let right = LogicalPlanBuilder::values(vec![vec![lit(1_i64)], vec![lit(1_i64)]])
                .unwrap()
                .alias("r")
                .unwrap()
                .distinct()
                .unwrap()
                .build()
                .unwrap();
            let condition = Expr::Column(left.schema().columns()[0].clone())
                .eq(Expr::Column(right.schema().columns()[0].clone()));
            let joined = LogicalPlanBuilder::from(left)
                .join_on(right, kind, vec![condition])
                .unwrap()
                .build()
                .unwrap();
            let optimized = simplify_existence(joined.clone()).unwrap();
            assert!(!optimized.display_indent().to_string().contains("Distinct"));
            for plan in [joined, optimized] {
                let result = SessionContext::new()
                    .execute_logical_plan(plan)
                    .await
                    .unwrap()
                    .collect()
                    .await
                    .unwrap();
                assert_eq!(
                    result.iter().map(|batch| batch.num_rows()).sum::<usize>(),
                    expected
                );
            }
        }
    }
}
