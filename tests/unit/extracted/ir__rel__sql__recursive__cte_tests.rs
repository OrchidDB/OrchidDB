
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

#[test]
fn semi_and_anti_joins_keep_filtered_and_grouped_preserved_inputs() {
    use datafusion::logical_expr::JoinType;
    let scan = |name: &str| {
        let schema = Arc::new(Schema::new(vec![Field::new("n",DataType::Int64,false)]));
        LogicalPlanBuilder::scan(name,provider_as_source(Arc::new(EmptyTable::new(schema))),None).unwrap().build().unwrap()
    };
    let filtered = LogicalPlanBuilder::from(scan("numbers")).filter(col("numbers.n").gt(lit(1i64))).unwrap().build().unwrap();
    let grouped = LogicalPlanBuilder::from(filtered.clone()).aggregate(vec![col("numbers.n")],Vec::<Expr>::new()).unwrap().build().unwrap();
    let mixed = LogicalPlanBuilder::from(scan("numbers"))
        .project(vec![col("numbers.n"),(col("numbers.n") * lit(10i64)).alias_qualified(Some("other"),"scaled")]).unwrap()
        .filter(col("numbers.n").gt(lit(1i64))).unwrap().build().unwrap();
    let unqualified = LogicalPlanBuilder::from(scan("numbers"))
        .project(vec![col("numbers.n").alias("n")]).unwrap()
        .filter(col("n").gt(lit(1i64))).unwrap().build().unwrap();
    let db=duckdb::Connection::open_in_memory().unwrap();
    db.execute_batch("CREATE TABLE numbers(n BIGINT); INSERT INTO numbers VALUES (1),(2),(3); CREATE TABLE selected(n BIGINT); INSERT INTO selected VALUES (1),(2),(2)").unwrap();
    for left in [filtered,grouped,mixed,unqualified] {
        for (kind,expected) in [(JoinType::LeftSemi,vec![2i64]),(JoinType::LeftAnti,vec![3i64])] {
            let right = if left.schema().iter().all(|(qualifier, _)| qualifier.is_none()) {
                LogicalPlanBuilder::from(scan("selected"))
                    .project(vec![col("selected.n").alias("key")]).unwrap()
                    .alias("selected_keys").unwrap().build().unwrap()
            } else { scan("selected") };
            let predicate = Expr::Column(left.schema().columns()[0].clone())
                .eq(Expr::Column(right.schema().columns()[0].clone()));
            let plan=LogicalPlanBuilder::from(left.clone()).join_on(right,kind,vec![predicate]).unwrap().build().unwrap();
            let (rewritten,ctes,_) = extract_ctes(plan.clone(),false).unwrap();
            assert_eq!(rewritten.schema(),plan.schema());
            assert!(ctes.iter().any(|cte|cte.name.contains("existence_left_filter")));
            for dialect in [SqlDialect::DuckDb,SqlDialect::Postgres,
                #[cfg(feature="trino")]
                SqlDialect::Custom(&super::super::trino::TRINO)] {
                let sql=unparse_plan(plan.clone(),dialect).unwrap();
                assert!(sql.to_ascii_lowercase().contains("exists"),"{dialect:?}: {sql}");
                if matches!(dialect,SqlDialect::DuckDb) {
                    let mut statement=db.prepare(&sql).unwrap_or_else(|e|panic!("{e}: {sql}"));
                    let actual=statement.query_map([],|row|row.get::<_,i64>(0)).unwrap().collect::<duckdb::Result<Vec<_>>>().unwrap();
                    assert_eq!(actual,expected,"{sql}");
                }
            }
        }
    }
}

#[test]
fn inlined_recursive_filter_scopes_keep_columns_from_both_join_inputs() {
    use datafusion::logical_expr::JoinType;
    let schema=|name: &str| Arc::new(Schema::new(vec![Field::new(name.to_owned(),DataType::Int64,false)]));
    let scan=|name: &str, field: &str| LogicalPlanBuilder::scan(name,
        provider_as_source(Arc::new(EmptyTable::new(schema(field)))),None).unwrap().build().unwrap();
    let joined=LogicalPlanBuilder::from(scan("r","step")).join_on(scan("increments","delta"),JoinType::Inner,vec![lit(true)])
        .unwrap().filter(col("delta").gt(lit(0_i64))).unwrap().build().unwrap();
    let work=LogicalPlanBuilder::scan("__w_sql_cte_join_filter_inline",
        provider_as_source(Arc::new(EmptyTable::new(Arc::new(joined.schema().as_arrow().clone())))),None).unwrap()
        .project(vec![(col("__w_sql_cte_join_filter_inline.step")+col("__w_sql_cte_join_filter_inline.delta")).alias("step")]).unwrap().build().unwrap();
    let seed=LogicalPlanBuilder::empty(true).project(vec![lit(0_i64).alias("step")]).unwrap().build().unwrap();
    let mut recursive=vec![RecursiveCte {name:"r".into(),static_term:seed,recursive_term:work,is_distinct:false}];
    inline_recursive_dependencies(&[PlainCte {name:"__w_sql_cte_join_filter_inline".into(),term:joined}],&mut recursive).unwrap();
    let db=duckdb::Connection::open_in_memory().unwrap();
    db.execute_batch("CREATE TABLE r(step BIGINT); INSERT INTO r VALUES (0),(1); CREATE TABLE increments(delta BIGINT); INSERT INTO increments VALUES (1),(0)").unwrap();
    for repair_scopes in [false,true] {
        let dialect=SqlDialect::DuckDb;
        let sql_dialect=dialect.unparser_dialect();
        let unparser=Unparser::new(sql_dialect.as_ref());
        let sql=unparse_one(&recursive[0].recursive_term,&unparser,dialect,repair_scopes).unwrap();
        let mut statement=db.prepare(&sql).unwrap_or_else(|error|panic!("repair={repair_scopes}: {error}: {sql}"));
        let mut values=statement.query_map([],|row|row.get::<_,i64>(0)).unwrap().collect::<duckdb::Result<Vec<_>>>().unwrap();
        values.sort();assert_eq!(values,vec![1,2],"repair={repair_scopes}: {sql}");
    }
}
