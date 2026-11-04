
    use super::*;
    use arrow::datatypes::{DataType, Field, Schema};
    use datafusion::datasource::empty::EmptyTable;
    use datafusion::logical_expr::LogicalPlanBuilder;
    use datafusion::prelude::{col, lit};

    #[test]
    fn conflicting_named_ctes_keep_each_filter() {
        let schema = Arc::new(Schema::new(vec![Field::new("n", DataType::Int64, false)]));
        let source = provider_as_source(Arc::new(EmptyTable::new(schema)));
        let input = LogicalPlanBuilder::scan("numbers", source, None).unwrap().build().unwrap();
        let branch = |n| LogicalPlanBuilder::from(input.clone())
            .filter(col("n").eq(lit(n))).unwrap()
            .alias("__w_sql_cte_shared").unwrap().build().unwrap();
        let plan = LogicalPlanBuilder::from(branch(1_i64)).union(branch(2_i64)).unwrap().build().unwrap();
        let sql = unparse_plan(plan, SqlDialect::DuckDb).unwrap();
        let db = duckdb::Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE numbers(n BIGINT); INSERT INTO numbers VALUES (1),(2),(3)").unwrap();
        let mut statement = db.prepare(&sql).unwrap();
        let mut values = statement.query_map([], |row| row.get::<_, i64>(0)).unwrap()
            .collect::<duckdb::Result<Vec<_>>>().unwrap();
        values.sort();
        assert_eq!(values, vec![1, 2], "{sql}");
    }

    #[test]
    fn identical_named_ctes_share_one_definition() {
        let schema = Arc::new(Schema::new(vec![Field::new("n", DataType::Int64, false)]));
        let source = provider_as_source(Arc::new(EmptyTable::new(schema)));
        let input = LogicalPlanBuilder::scan("numbers", source, None).unwrap()
            .alias("__w_sql_cte_shared").unwrap().build().unwrap();
        let plan = LogicalPlanBuilder::from(input.clone()).union(input).unwrap().build().unwrap();
        let (_, definitions, _) = extract_ctes(plan, false).unwrap();
        assert_eq!(definitions.len(), 1);
    }
