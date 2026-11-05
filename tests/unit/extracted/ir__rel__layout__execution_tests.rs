
    use super::*;
    use arrow::{
        array::Int64Array,
        datatypes::{DataType, Field, Schema},
        record_batch::RecordBatch,
    };
    use datafusion::{
        datasource::MemTable,
        prelude::{col, lit},
    };
    #[tokio::test]
    async fn selected_scan_preserves_duplicates_and_nulls() {
        let schema = Arc::new(Schema::new(vec![Field::new("id", DataType::Int64, true)]));
        let mut providers = BTreeMap::new();
        for (name, values) in [
            ("first", vec![Some(1), None, Some(2), Some(2)]),
            ("second", vec![Some(2), Some(1), Some(2), None]),
        ] {
            let batch =
                RecordBatch::try_new(schema.clone(), vec![Arc::new(Int64Array::from(values))])
                    .unwrap();
            providers.insert(
                name.into(),
                Arc::new(MemTable::try_new(schema.clone(), vec![vec![batch]]).unwrap())
                    as Arc<dyn TableProvider>,
            );
        }
        let source: LogicalSource = serde_json::from_value(serde_json::json!({"name":"logical", "default_table":"first", "layouts":[
            {"table":"first", "specs":[{"spec_id":0,"fields":[]}], "partitions":[{"spec_id":0,"bytes":100,"files":1}]},
            {"table":"second", "specs":[{"spec_id":0,"fields":[]}], "partitions":[{"spec_id":0,"bytes":50,"files":1}]}
        ]})).unwrap();
        let provider = Arc::new(LayoutProvider::try_new(source, &providers).unwrap());
        let original = LogicalPlanBuilder::scan("logical", provider_as_source(provider), None)
            .unwrap()
            .filter(col("id").eq(lit(2_i64)).or(col("id").is_null()))
            .unwrap()
            .build()
            .unwrap();
        let (plan, choices) = select(original).unwrap();
        assert_eq!(choices[0].table, "second");
        let batches = crate::ir::rel::execute_test_plan(plan).await.unwrap();
        assert_eq!(batches.iter().map(|b| b.num_rows()).sum::<usize>(), 3);
        assert_eq!(
            batches
                .iter()
                .map(|b| b.column(0).null_count())
                .sum::<usize>(),
            1
        );
    }

#[tokio::test]
async fn projected_layout_scan_preserves_column_order_residuals_and_ordinary_union_input() {
    use arrow::array::{ArrayRef,StringArray};
    let schema=Arc::new(Schema::new(vec![Field::new("id",DataType::Int64,true),Field::new("tag",DataType::Utf8,true)]));
    let batch=RecordBatch::try_new(schema.clone(),vec![
        Arc::new(Int64Array::from(vec![Some(2),None,Some(1)])) as ArrayRef,
        Arc::new(StringArray::from(vec!["same","nil","one"])) as ArrayRef,
    ]).unwrap();
    let table=Arc::new(MemTable::try_new(schema,vec![vec![batch]]).unwrap()) as Arc<dyn TableProvider>;
    let providers=BTreeMap::from([("first".into(),table.clone()),("cheap".into(),table.clone())]);
    let definition=serde_json::from_value(serde_json::json!({"name":"logical","default_table":"first","layouts":[
        {"table":"first","specs":[{"spec_id":0,"fields":[]}],"partitions":[{"spec_id":0,"bytes":100,"files":1}]},
        {"table":"cheap","specs":[{"spec_id":0,"fields":[]}],"partitions":[{"spec_id":0,"bytes":10,"files":1}]}
    ]})).unwrap();
    let layout=Arc::new(LayoutProvider::try_new(definition,&providers).unwrap());
    let ordinary=LogicalPlanBuilder::scan("ordinary",provider_as_source(table),Some(vec![1,0])).unwrap().build().unwrap();
    let original=LogicalPlanBuilder::scan("logical",provider_as_source(layout),Some(vec![1,0])).unwrap()
        .filter(col("id").gt_eq(lit(1_i64))).unwrap().union(ordinary).unwrap().build().unwrap();
    let (selected,choices)=select(original).unwrap();
    assert_eq!(choices.len(),1);assert_eq!(choices[0].table,"cheap");
    assert_eq!(selected.schema().fields().iter().map(|f|f.name().as_str()).collect::<Vec<_>>(),["tag","id"]);
    let batches=crate::ir::rel::execute_test_plan(selected).await.unwrap();
    let mut rows=Vec::new();
    for batch in batches {for row in 0..batch.num_rows(){rows.push(batch.columns().iter().map(|a|arrow::util::display::array_value_to_string(a,row).unwrap()).collect::<Vec<_>>().join("|"));}}
    rows.sort();assert_eq!(rows,["nil|","one|1","one|1","same|2","same|2"]);
}
