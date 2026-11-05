
    use super::*;
    use std::ops::Not;
    #[test]
    fn literal_branch_specialization_keeps_null_semantics() {
        assert_eq!(literals(lit("A").eq(lit("B"))).unwrap(),lit(false));
        assert_eq!(literals(lit(ScalarValue::Utf8(None)).eq(lit("B"))).unwrap(),lit(ScalarValue::Boolean(None)));
    }
    #[test]
    fn filter_crosses_renames_but_not_limits() {
        let base=LogicalPlanBuilder::empty(true).project(vec![lit(1_i64).alias("x")]).unwrap().build().unwrap();
        let plan=LogicalPlanBuilder::from(base.clone()).project(vec![datafusion::prelude::col("x").alias("y")]).unwrap()
            .filter(datafusion::prelude::col("y").eq(lit(1_i64))).unwrap().build().unwrap();
        let optimized=rewrite(plan).unwrap();
        assert!(matches!(optimized,LogicalPlan::Projection(_)));
        let plan=LogicalPlanBuilder::from(base).limit(0,Some(1)).unwrap()
            .filter(datafusion::prelude::col("x").eq(lit(1_i64))).unwrap().build().unwrap();
        assert_eq!(rewrite(plan.clone()).unwrap(),plan);
    }

#[test]
fn literal_predicates_keep_three_valued_boolean_and_typed_equality_semantics() {
    use datafusion::prelude::col;
    for value in [None, Some(false), Some(true)] {
        let e = lit(ScalarValue::Boolean(value));
        assert_eq!(literals(e.clone().not()).unwrap(), lit(ScalarValue::Boolean(value.map(|v| !v))));
        assert_eq!(literals(e.clone().is_null()).unwrap(), lit(value.is_none()));
        assert_eq!(literals(e.clone().is_not_null()).unwrap(), lit(value.is_some()));
        for constant in [false, true] {
            for reversed in [false, true] {
                let (left, right) = if reversed { (lit(constant), e.clone()) } else { (e.clone(), lit(constant)) };
                assert_eq!(literals(left.clone().and(right.clone())).unwrap(), if constant { e.clone() } else { lit(false) });
                assert_eq!(literals(left.or(right)).unwrap(), if constant { lit(true) } else { e.clone() });
            }
        }
    }
    for value in [ScalarValue::Int8(Some(7)), ScalarValue::Int16(Some(7)), ScalarValue::Int32(Some(7)),
        ScalarValue::Int64(Some(7)), ScalarValue::UInt8(Some(7)), ScalarValue::UInt16(Some(7)),
        ScalarValue::UInt32(Some(7)), ScalarValue::UInt64(Some(7)), ScalarValue::Utf8(Some("雪".into())),
        ScalarValue::LargeUtf8(Some("雪".into())), ScalarValue::Utf8View(Some("雪".into()))] {
        let null = ScalarValue::try_from(&value.data_type()).unwrap();
        assert_eq!(literals(lit(value.clone()).eq(lit(value.clone()))).unwrap(), lit(true));
        assert_eq!(literals(lit(value.clone()).not_eq(lit(value.clone()))).unwrap(), lit(false));
        assert_eq!(literals(lit(value).not_eq(lit(null))).unwrap(), lit(ScalarValue::Boolean(None)));
    }
    for expression in [col("x").is_null(), col("x").is_not_null(), col("x").not(),
        lit(1_i32).eq(lit(1_i64)), lit(1_f64).eq(lit(1_f64)), lit(1_i64).lt(lit(2_i64))] {
        assert_eq!(literals(expression.clone()).unwrap(), expression);
    }
}

#[test]
fn predicate_scope_only_substitutes_total_aliases_and_typed_nulls() {
    use datafusion::prelude::col;
    let plan = LogicalPlanBuilder::empty(true).project(vec![lit(7_i64).alias("x")]).unwrap().build().unwrap();
    let schema = plan.schema();
    for expression in [col("x"), lit(1_i64), col("x").alias("renamed"), col("x").is_null(),
        col("x").is_not_null(), col("x").eq(lit(1_i64)).not(),
        col("x").gt_eq(lit(1_i64)).and(col("x").lt_eq(lit(2_i64)))] {
        assert!(total(&expression, schema), "{expression}");
    }
    for expression in [col("missing").eq(lit(1_i64)), col("x").eq(lit("1")), col("x") + lit(1_i64)] {
        assert!(!total(&expression, schema), "{expression}");
    }
    let null = datafusion::logical_expr::expr_fn::cast(lit(ScalarValue::Null), DataType::Int64);
    assert!(value(&null));
    assert_eq!(unalias(null.alias("a").alias("b")), lit(ScalarValue::Int64(None)));
    assert!(!value(&(col("x") + lit(1_i64))));
    assert_eq!(filtered(plan.clone(), lit(true)).unwrap(), plan);
}

fn scope_values(name: &str, values: &[Option<i64>]) -> LogicalPlan {
    use crate::ir::rel::constraints::{bind, Constraint, ConstraintCatalog, Fact};
    use datafusion::datasource::{MemTable, provider_as_source};
    let schema = Arc::new(arrow::datatypes::Schema::new(vec![
        Field::new("row_id", DataType::Int64, false), Field::new("x", DataType::Int64, true)]));
    let batch = RecordBatch::try_new(schema.clone(), vec![
        Arc::new(arrow::array::Int64Array::from_iter_values(0..values.len() as i64)),
        Arc::new(arrow::array::Int64Array::from(values.to_vec()))]).unwrap();
    let table = Arc::new(MemTable::try_new(schema, vec![vec![batch]]).unwrap());
    let mut catalog = ConstraintCatalog::default();
    catalog.insert(name, Constraint::enforced("row_id_present", Fact::NonNull { columns: vec!["row_id".into()] }));
    let table = bind(name, table, &catalog, None).unwrap();
    LogicalPlanBuilder::scan(name, provider_as_source(table), None).unwrap()
        .project(vec![datafusion::prelude::col(format!("{name}.x"))]).unwrap().build().unwrap()
}
async fn scope_rows(plan: LogicalPlan) -> Vec<Vec<String>> {
    let batches = crate::ir::rel::execute_test_plan(plan).await.unwrap();
    let mut rows = Vec::new();
    for batch in batches {
        for row in 0..batch.num_rows() {
            rows.push(batch.columns().iter().map(|array| if array.is_null(row) { "NULL".into() }
                else { arrow::util::display::array_value_to_string(array, row).unwrap() }).collect());
        }
    }
    rows.sort(); rows
}

#[tokio::test]
async fn total_predicate_movement_preserves_union_alias_filter_and_limit_results() {
    use datafusion::prelude::col;
    for wrapper in 0..5 {
        let base = scope_values("source", &[None, Some(1), Some(2), Some(2), Some(3)]);
        let (plan, name, expected_count) = match wrapper {
            0 => (LogicalPlanBuilder::from(base).project(vec![col("source.x").alias("renamed")]).unwrap().build().unwrap(), "renamed", 3),
            1 => (LogicalPlanBuilder::from(base).alias("renamed").unwrap().build().unwrap(), "renamed.x", 3),
            2 => (LogicalPlanBuilder::from(base).filter(col("source.x").is_not_null()).unwrap().build().unwrap(), "source.x", 3),
            3 => (LogicalPlanBuilder::from(base).project(vec![col("source.x").alias("x")]).unwrap().union(LogicalPlanBuilder::from(scope_values("other", &[Some(2), None])).project(vec![col("other.x").alias("x")]).unwrap().build().unwrap()).unwrap().build().unwrap(), "x", 4),
            _ => (LogicalPlanBuilder::from(base).limit(0, Some(2)).unwrap().build().unwrap(), "source.x", 0),
        };
        let original = LogicalPlanBuilder::from(plan).filter(col(name).gt(lit(1_i64))).unwrap().build().unwrap();
        let optimized = rewrite(original.clone()).unwrap();
        let actual = scope_rows(optimized.clone()).await;
        assert_eq!(actual, scope_rows(original.clone()).await, "wrapper {wrapper}");
        assert_eq!(actual.len(), expected_count, "wrapper {wrapper}");
        if wrapper == 4 { assert_eq!(optimized, original); }
        else { assert_ne!(optimized, original, "wrapper {wrapper} did not exercise movement"); }
    }
}

#[tokio::test]
async fn join_predicate_partitioning_keeps_mixed_terms_duplicates_and_outer_nulls() {
    use datafusion::prelude::col;
    for join_type in [JoinType::Inner, JoinType::Left] {
        let left = scope_values("l", &[None, Some(1), Some(2), Some(2), Some(4)]);
        let right = scope_values("r", &[Some(1), Some(2), Some(2), Some(3)]);
        let joined = LogicalPlanBuilder::from(left).join(right, join_type,
            (vec!["l.x"], vec!["r.x"]), None).unwrap().build().unwrap();
        let predicate = col("l.x").gt(lit(1_i64)).and(col("r.x").lt(lit(4_i64)))
            .and(col("l.x").eq(col("r.x")));
        let original = LogicalPlanBuilder::from(joined).filter(predicate).unwrap().build().unwrap();
        let optimized = rewrite(original.clone()).unwrap();
        let actual = scope_rows(optimized.clone()).await;
        assert_eq!(actual, vec![vec!["2".to_string(), "2".to_string()]; 4]);
        assert_eq!(actual, scope_rows(original.clone()).await);
        if join_type == JoinType::Inner { assert_ne!(optimized, original); }
        else { assert_eq!(optimized, original); }
    }
}

#[tokio::test]
async fn join_specialization_propagates_non_null_constants_from_either_side() {
    use datafusion::prelude::col;
    for constant_on_left in [false, true] {
        for value in [Some(2_i64), None] {
            let constant = LogicalPlanBuilder::from(scope_values("constant_source", &[Some(0)])).project(vec![lit(ScalarValue::Int64(value)).alias("x")]).unwrap()
                .alias("c").unwrap().build().unwrap();
            let dynamic = scope_values("d", &[None, Some(1), Some(2), Some(2), Some(3)]);
            let (left, right, left_key, right_key) = if constant_on_left { (constant, dynamic, "c.x", "d.x") }
                else { (dynamic, constant, "d.x", "c.x") };
            let original = LogicalPlanBuilder::from(left).join(right, JoinType::Inner,
                (vec![left_key], vec![right_key]), None).unwrap().build().unwrap();
            let specialized = specialize(original.clone()).unwrap();
            assert_eq!(specialized.transformed, value.is_some());
            let actual = scope_rows(specialized.data).await;
            assert_eq!(actual, scope_rows(original).await);
            assert_eq!(actual.len(), if value.is_some() { 2 } else { 0 });
        }
    }
    let same = LogicalPlanBuilder::empty(true).project(vec![lit(7_i64).alias("x")]).unwrap().build().unwrap();
    let union = LogicalPlanBuilder::from(same.clone()).union(same).unwrap().build().unwrap();
    assert_eq!(constant(&union, &Column::from_name("x")), Some(ScalarValue::Int64(Some(7))));
    let different = LogicalPlanBuilder::empty(true).project(vec![lit(8_i64).alias("x")]).unwrap().build().unwrap();
    let union = LogicalPlanBuilder::from(union).union(different).unwrap().build().unwrap();
    assert_eq!(constant(&union, &Column::from_name("x")), None);
    assert_eq!(constant(&union, &Column::from_name("missing")), None);
    assert!(!total(&(col("x") / lit(0_i64)), union.schema()));
}
