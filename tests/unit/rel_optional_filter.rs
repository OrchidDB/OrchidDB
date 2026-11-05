use super::*;
use arrow::array::Int64Array;
use arrow::datatypes::{Field, Schema};
use datafusion::{
    datasource::{MemTable, provider_as_source},
    logical_expr::{LogicalPlanBuilder, col},
};

fn scan(name: &str, fields: Vec<Field>, columns: Vec<Vec<Option<i64>>>) -> LogicalPlan {
    let schema = Arc::new(Schema::new(fields));
    let batch = RecordBatch::try_new(
        schema.clone(),
        columns
            .into_iter()
            .map(|values| Arc::new(Int64Array::from(values)) as arrow::array::ArrayRef)
            .collect(),
    )
    .unwrap();
    let table = MemTable::try_new(schema, vec![vec![batch]]).unwrap();
    LogicalPlanBuilder::scan(name, provider_as_source(Arc::new(table)), None)
        .unwrap()
        .build()
        .unwrap()
}
fn joined() -> LogicalPlan {
    let left = scan(
        "l",
        vec![Field::new("parent", DataType::Int64, false)],
        vec![vec![Some(1), Some(2), Some(3)]],
    );
    let right = scan(
        "r",
        vec![
            Field::new("id", DataType::Int64, false),
            Field::new("value", DataType::Int64, true),
        ],
        vec![vec![Some(1), Some(1), Some(2)], vec![Some(7), None, None]],
    );
    LogicalPlanBuilder::from(left)
        .join(
            right,
            JoinType::Left,
            (vec!["l.parent"], vec!["r.id"]),
            None,
        )
        .unwrap()
        .build()
        .unwrap()
}
async fn rows(plan: LogicalPlan) -> Vec<Vec<String>> {
    let batches = crate::ir::rel::execute_test_plan(plan).await.unwrap();
    let mut rows = batches
        .iter()
        .flat_map(|batch| {
            (0..batch.num_rows()).map(|row| {
                batch
                    .columns()
                    .iter()
                    .map(|array| {
                        if array.is_null(row) {
                            "NULL".into()
                        } else {
                            arrow::util::display::array_value_to_string(array, row).unwrap()
                        }
                    })
                    .collect()
            })
        })
        .collect::<Vec<Vec<String>>>();
    rows.sort();
    rows
}

#[tokio::test]
async fn optional_presence_rewrite_preserves_duplicates_nullable_properties_and_aliases() {
    for property in [false, true] {
        for absent in [false, true] {
            for wrapper in 0..3 {
                let mut plan = joined();
                let mut target = col(if property { "r.value" } else { "r.id" });
                if wrapper > 0 {
                    plan = LogicalPlanBuilder::from(plan)
                        .project(vec![
                            col("l.parent"),
                            col("r.id"),
                            col("r.value"),
                            target.alias("present"),
                        ])
                        .unwrap()
                        .build()
                        .unwrap();
                    target = col("present");
                }
                if wrapper == 2 {
                    plan = LogicalPlanBuilder::from(plan)
                        .alias("renamed")
                        .unwrap()
                        .build()
                        .unwrap();
                    target = col("renamed.present");
                }
                let condition = if absent {
                    target.is_null()
                } else {
                    target.is_not_null()
                };
                let original = LogicalPlanBuilder::from(plan)
                    .filter(condition)
                    .unwrap()
                    .build()
                    .unwrap();
                let rewritten = rewrite(original.clone()).unwrap();
                assert_eq!(rewritten != original, !(property && absent));
                let expected = rows(original).await;
                assert_eq!(
                    expected.len(),
                    match (property, absent) {
                        (false, false) => 3,
                        (false, true) => 1,
                        (true, false) => 1,
                        (true, true) => 3,
                    }
                );
                assert_eq!(
                    rows(rewritten).await,
                    expected,
                    "property={property}, absent={absent}, wrapper={wrapper}"
                );
            }
        }
    }
}

#[test]
fn optional_presence_does_not_move_past_computed_aliases_or_test_outer_columns() {
    for plan in [
        LogicalPlanBuilder::from(joined())
            .filter(col("l.parent").is_null())
            .unwrap()
            .build()
            .unwrap(),
        LogicalPlanBuilder::from(joined())
            .project(vec![(lit(1_i64) / col("r.value")).alias("computed")])
            .unwrap()
            .filter(col("computed").is_null())
            .unwrap()
            .build()
            .unwrap(),
        LogicalPlanBuilder::from(joined())
            .project(vec![lit(1_i64).alias("constant")])
            .unwrap()
            .filter(col("constant").is_not_null())
            .unwrap()
            .build()
            .unwrap(),
    ] {
        assert_eq!(rewrite(plan.clone()).unwrap(), plan);
    }
}

#[test]
fn optional_presence_safety_accepts_only_total_scalar_expressions() {
    use datafusion::logical_expr::expr::{Cast,TryCast};
    let schema=datafusion::common::DFSchema::try_from(Schema::new(vec![
        Field::new("small",DataType::Int16,true),Field::new("wide",DataType::Int64,true),Field::new("text",DataType::Utf8,true)
    ])).unwrap();
    for expr in [col("small"),lit(7_i64),col("wide").alias("renamed"),
        Expr::Cast(Cast::new(Box::new(col("small")),DataType::Int64)),
        Expr::Cast(Cast::new(Box::new(col("wide")),DataType::Int64)),
        col("wide").eq(lit(7_i64)),col("wide").not_eq(lit(7_i64)),
        col("wide").is_null().or(col("small").is_not_null()),
        col("wide").is_not_null().and(col("small").is_null())] {
        assert!(total(&expr,&schema),"total expression rejected: {expr}");
    }
    for expr in [Expr::Cast(Cast::new(Box::new(col("text")),DataType::Int64)),
        Expr::Cast(Cast::new(Box::new(col("wide")),DataType::Int16)),
        Expr::Cast(Cast::new(Box::new(col("missing")),DataType::Int64)),
        Expr::TryCast(TryCast::new(Box::new(col("text")),DataType::Int64)),
        col("wide")/lit(0_i64),(col("wide")/lit(0_i64)).alias("computed"),
        (col("wide")/lit(0_i64)).eq(lit(1_i64)),
        lit(1_i64).eq(col("wide")/lit(0_i64)),col("wide").gt(lit(1_i64))] {
        assert!(!total(&expr,&schema),"unproven expression accepted: {expr}");
    }
}

#[tokio::test]
async fn optional_presence_rewrite_preserves_union_fanout_and_respects_right_side_barriers() {
    let LogicalPlan::Join(join)=joined() else {panic!()};
    let right=join.right.as_ref().clone();
    let union=LogicalPlanBuilder::from(right.clone()).union(right.clone()).unwrap().alias("r").unwrap().build().unwrap();
    let filtered=LogicalPlanBuilder::from(right.clone()).filter(col("r.id").eq(lit(1_i64)).or(col("r.id").eq(lit(2_i64)))).unwrap().build().unwrap();
    let projected=LogicalPlanBuilder::from(right.clone()).project(vec![col("r.id"),col("r.value")]).unwrap().build().unwrap();
    let limited=LogicalPlanBuilder::from(right.clone()).limit(0,Some(1)).unwrap().build().unwrap();
    let sorted=LogicalPlanBuilder::from(right.clone()).sort(vec![col("r.id").sort(true,false)]).unwrap().build().unwrap();
    let unsafe_projection=LogicalPlanBuilder::from(right).project(vec![col("r.id"),(lit(1_i64)/col("r.value")).alias("value")]).unwrap().build().unwrap();
    for (right,can_rewrite) in [(union,true),(filtered,true),(projected,true),(limited,false),(sorted,false),(unsafe_projection,false)] {
        assert_eq!(safe(&right),can_rewrite);
        for absent in [false,true] {
            let right_key=right.schema().columns()[0].clone();
            let left_key=join.left.schema().columns()[0].clone();
            let plan=LogicalPlanBuilder::from(join.left.as_ref().clone())
                .join(right.clone(),JoinType::Left,(vec![left_key],vec![right_key.clone()]),None).unwrap()
                .filter(if absent {Expr::Column(right_key).is_null()} else {Expr::Column(right_key).is_not_null()}).unwrap().build().unwrap();
            let rewritten=rewrite(plan.clone()).unwrap();
            assert_eq!(rewritten!=plan,can_rewrite);
            assert_eq!(rows(rewritten).await,rows(plan).await);
        }
    }
}
