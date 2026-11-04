
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
