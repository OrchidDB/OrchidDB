
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
        let mut statement = db.prepare(&sql).unwrap_or_else(|error| panic!("{error}: {sql}"));
        let mut values = statement.query_map([], |row| row.get::<_, i64>(0)).unwrap()
            .collect::<duckdb::Result<Vec<_>>>().unwrap_or_else(|error| panic!("{error}: {sql}"));
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

#[test]
fn aliased_scalar_aggregates_preserve_values_and_cardinality() {
    use datafusion::functions_aggregate::expr_fn::sum;
    use datafusion::logical_expr::JoinType;
    let schema = Arc::new(Schema::new(vec![Field::new("n", DataType::Int64, false)]));
    let batch = arrow::array::RecordBatch::try_new(schema.clone(), vec![
        Arc::new(arrow::array::Int64Array::from(vec![2, 3, 7]))]).unwrap();
    let source = provider_as_source(Arc::new(datafusion::datasource::MemTable::try_new(schema, vec![vec![batch]]).unwrap()));
    let input = LogicalPlanBuilder::scan("numbers", source, None).unwrap().build().unwrap();
    let aggregate = LogicalPlanBuilder::from(input.clone())
        .aggregate(Vec::<Expr>::new(), vec![sum(col("n")).alias("total")]).unwrap()
        .alias("scalar_inner").unwrap().alias("scalar_outer").unwrap().build().unwrap();
    let joined = LogicalPlanBuilder::from(input).join_on(aggregate.clone(), JoinType::Left, vec![lit(true)])
        .unwrap().project(vec![col("total")]).unwrap().build().unwrap();
    for (plan, row_count) in [(aggregate, 1), (joined, 3)] {
        for dialect in [SqlDialect::DuckDb, SqlDialect::Postgres,
            #[cfg(feature = "trino")]
            SqlDialect::Custom(&super::super::trino::TRINO)] {
            let sql = unparse_plan(plan.clone(), dialect).unwrap();
            assert!(sql.to_lowercase().contains("sum("), "{dialect:?}: {sql}");
            if matches!(dialect, SqlDialect::DuckDb) {
                let db = duckdb::Connection::open_in_memory().unwrap();
                db.execute_batch("CREATE TABLE numbers(n BIGINT); INSERT INTO numbers VALUES (2),(3),(7)").unwrap();
                let mut statement = db.prepare(&sql).unwrap_or_else(|error| panic!("{error}: {sql}"));
                let values = statement.query_map([], |row| row.get::<_, i64>(0)).unwrap()
                    .collect::<duckdb::Result<Vec<_>>>().unwrap_or_else(|error| panic!("{error}: {sql}"));
                assert_eq!(values, vec![12; row_count], "{sql}");
            }
        }
    }
}
