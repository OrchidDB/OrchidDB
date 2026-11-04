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
