
    use super::*;
    use datafusion::logical_expr::{ExprFunctionExt, LogicalPlanBuilder};
    use datafusion::prelude::col;

    #[test]
    fn representation_casts_keep_declared_projection_names() {
        use datafusion::logical_expr::{Projection, expr::Cast};
        let input = LogicalPlanBuilder::empty(true).project(vec![lit("IRI").alias("kind")]).unwrap().build().unwrap();
        let schema = input.schema().clone();
        let expression = Expr::Cast(Cast::new(Box::new(col("kind")), DataType::Utf8));
        let plan = LogicalPlan::Projection(Projection::try_new_with_schema(vec![expression], Arc::new(input), schema).unwrap());
        let fixed = normalize_projection_names(plan).unwrap();
        let LogicalPlan::Projection(projection) = fixed else { panic!() };
        assert_eq!(projection.expr[0].to_string(), "CAST(kind AS Utf8) AS kind");
        assert_eq!(projection.schema.field(0).name(), "kind");
    }

    #[test]
    fn repeated_casts_preserve_inner_failure_semantics_and_type_changes() {
        use datafusion::logical_expr::expr::{Cast, TryCast};
        for inner_try in [false, true] {
            for outer_try in [false, true] {
                let inner = if inner_try {
                    Expr::TryCast(TryCast::new(Box::new(lit("invalid")), DataType::Int64))
                } else {
                    Expr::Cast(Cast::new(Box::new(lit("invalid")), DataType::Int64))
                };
                for ty in [DataType::Int64, DataType::Int32] {
                    let expression = if outer_try {
                        Expr::TryCast(TryCast::new(Box::new(inner.clone()), ty.clone()))
                    } else {
                        Expr::Cast(Cast::new(Box::new(inner.clone()), ty.clone()))
                    };
                    let plan = LogicalPlanBuilder::empty(true).project(vec![expression.clone().alias("v")]).unwrap().build().unwrap();
                    let LogicalPlan::Projection(fixed) = normalize_projection_names(plan).unwrap() else { panic!() };
                    let expected = if ty == DataType::Int64 { inner.clone() } else { expression };
                    assert_eq!(fixed.expr[0], expected.alias("v"));
                }
            }
        }
    }

    use datafusion::functions_aggregate::expr_fn::array_agg;
    use datafusion::sql::sqlparser::{dialect::GenericDialect, parser::Parser};

    fn plan(aggregates: Vec<Expr>) -> LogicalPlan {
        LogicalPlanBuilder::empty(true)
            .project(vec![lit(1_i64).alias("x"),lit(2_i64).alias("k")]).unwrap()
            .aggregate(Vec::<Expr>::new(), aggregates).unwrap().build().unwrap()
    }
    fn repair(plan: &LogicalPlan, sql: &str) -> SqlResult<String> {
        let dialect = SqlDialect::DuckDb.unparser_dialect();
        let unparser = Unparser::new(dialect.as_ref());
        let statement = Parser::parse_sql(&GenericDialect, sql).unwrap().remove(0);
        restore_aggregate_ordering(plan, &unparser, SqlDialect::DuckDb, statement).map(|statement| statement.to_string())
    }
    fn ordered(ascending: bool) -> Expr {
        array_agg(col("x")).order_by(vec![col("k").sort(ascending,false)]).build().unwrap()
    }
    #[test]
    fn repeated_ordered_calls_are_repaired_without_changing_literal_data() {
        let plan = plan(vec![ordered(true).alias("xs")]);
        let sql = repair(&plan, "SELECT CASE WHEN array_agg(\"x\") IS NULL THEN array_agg(\"x\") ELSE NULL END, 'array_agg(\"x\")'").unwrap();
        assert_eq!(sql.matches("array_agg(\"x\" ORDER BY \"k\" ASC NULLS LAST)").count(),2);
        assert!(sql.contains("'array_agg(\"x\")'"));
    }
    #[test]
    fn different_ordering_contracts_cannot_be_guessed_from_equal_call_syntax() {
        for second in [array_agg(col("x")), ordered(false)] {
            let plan = plan(vec![ordered(true).alias("a"),second.alias("b")]);
            assert!(repair(&plan,"SELECT array_agg(\"x\"), array_agg(\"x\")").is_err());
        }
    }
    #[test]
    fn conditionless_outer_joins_emit_an_explicit_true_predicate() {
        use datafusion::logical_expr::JoinType;
        for kind in [JoinType::Left, JoinType::Right, JoinType::Full] {
            let scan = |name: &str, column: &str| {
                let table = datafusion::datasource::empty::EmptyTable::new(Arc::new(
                    arrow::datatypes::Schema::new(vec![Field::new(column, DataType::Int64, true)])));
                LogicalPlanBuilder::scan(name, datafusion::datasource::provider_as_source(Arc::new(table)), None)
                    .unwrap().build().unwrap()
            };
            let left = scan("left_input", "left_value");
            let right = scan("right_input", "right_value");
            let plan = LogicalPlanBuilder::from(left)
                .join(right, kind, (Vec::<Column>::new(), Vec::<Column>::new()), Some(lit(true)))
                .unwrap().build().unwrap();
            // Reproduce the optimizer's conditionless join; the builder itself
            // rejects this shape when supplied directly by callers.
            let LogicalPlan::Join(mut join) = plan else { unreachable!() };
            join.filter = None;
            let sql = unparse_plan(LogicalPlan::Join(join), SqlDialect::DuckDb).unwrap();
            assert!(sql.contains("ON true"), "{sql}");
            Parser::parse_sql(&GenericDialect, &sql).unwrap();
        }
    }


#[test]
fn postgres_text_extrema_use_binary_collation_without_changing_native_values() {
    use datafusion::functions_aggregate::expr_fn::{min,max};
    use datafusion::logical_expr::{ColumnarValue,ScalarFunctionArgs};
    for ty in [DataType::Utf8,DataType::LargeUtf8,DataType::Utf8View] {
        let literal=|value:Option<&str>|lit(match ty {
            DataType::Utf8=>ScalarValue::Utf8(value.map(str::to_owned)),
            DataType::LargeUtf8=>ScalarValue::LargeUtf8(value.map(str::to_owned)),
            _=>ScalarValue::Utf8View(value.map(str::to_owned)),
        });
        let input=LogicalPlanBuilder::values(vec![vec![literal(Some("z"))],vec![literal(Some("ä"))],
            vec![literal(Some("A"))],vec![literal(None)]]).unwrap().build().unwrap();
        let column=input.schema().field(0).name().clone();
        let plan=LogicalPlanBuilder::from(input).aggregate(Vec::<Expr>::new(),vec![min(col(&column)).alias("lo"),max(col(&column)).alias("hi")]).unwrap().build().unwrap();
        let fixed=preserve_postgres_text_extrema(plan.clone(),SqlDialect::Postgres).unwrap();
        let debug=format!("{fixed:?}");
        assert!(debug.contains("__postgres_binary_text"),"{debug}");
        let unchanged=preserve_postgres_text_extrema(plan.clone(),SqlDialect::DuckDb).unwrap();
        assert_eq!(unchanged,plan);
        let LogicalPlan::Aggregate(aggregate)=fixed else {panic!()};
        for expression in aggregate.aggr_expr {
            let Expr::Alias(alias)=expression else {panic!()};
            let Expr::AggregateFunction(aggregate)=*alias.expr else {panic!()};
            let Expr::ScalarFunction(call)=&aggregate.params.args[0] else {panic!()};
            for value in [None,Some("ä"),Some("A")] {
                let Expr::Literal(value,_)=literal(value) else {panic!()};
                let output=call.func.invoke_with_args(ScalarFunctionArgs{
                    args:vec![ColumnarValue::Scalar(value.clone())],
                    arg_fields:vec![Arc::new(Field::new("value",ty.clone(),true))],number_rows:1,
                    return_field:Arc::new(Field::new("result",ty.clone(),true)),config_options:Default::default(),
                }).unwrap();
                let ColumnarValue::Scalar(actual)=output else {panic!()};
                assert_eq!(actual,value);
            }
        }
    }
}

#[test]
fn literal_encoding_rejects_postgres_nul_and_retains_null_collection_types() {
    use datafusion::logical_expr::ExprSchemable;
    use arrow::array::{Array,ArrayRef,new_null_array};
    for ty in [DataType::List(Arc::new(Field::new("item",DataType::Int64,true))),
        DataType::LargeList(Arc::new(Field::new("item",DataType::Int64,true))),
        DataType::FixedSizeList(Arc::new(Field::new("item",DataType::Int64,true)),2)] {
        let array:ArrayRef=new_null_array(&ty,1);
        let value=ScalarValue::try_from_array(array.as_ref(),0).unwrap();
        assert!(array.is_null(0));
        for dialect in [SqlDialect::DuckDb,SqlDialect::Postgres] {
            let output=encode_expression_literals(lit(value.clone()),dialect).unwrap().data;
            assert_eq!(output.get_type(&datafusion::common::DFSchema::empty()).unwrap(),ty);
        }
    }
    for value in [ScalarValue::Utf8(Some("a\0b".into())),ScalarValue::LargeUtf8(Some("\0".into())),ScalarValue::Utf8View(Some("x\0".into()))] {
        assert!(encode_expression_literals(lit(value),SqlDialect::Postgres).unwrap_err().to_string().contains("postgres text cannot contain a NUL character"));
    }
}

#[test]
fn constant_sort_removal_preserves_top_k_and_nonconstant_key_ordering() {
    for mixed in [false,true] {
        for fetch in [None,Some(0),Some(2)] {
            let input=LogicalPlanBuilder::values(vec![vec![lit(1_i64)],vec![lit(3_i64)],vec![lit(2_i64)]]).unwrap().build().unwrap();
            let name=input.schema().field(0).name().clone();
            let mut keys=vec![lit(ScalarValue::Null).sort(true,true)];
            if mixed {keys.push(col(&name).sort(false,false));}
            let plan=LogicalPlanBuilder::from(input).sort_with_limit(keys,fetch).unwrap().build().unwrap();
            let fixed=strip_constant_sorts(plan).unwrap();
            if let LogicalPlan::Sort(sort)=&fixed {
                assert!(mixed);
                assert_eq!(sort.expr.len(),1);
                assert_eq!(sort.fetch,fetch);
            } else {assert!(!mixed);}
            let sql=unparse_plan(fixed,SqlDialect::DuckDb).unwrap();
            let rows=DuckDbExecutor::new().run(&[],&sql).unwrap();
            assert_eq!(rows.len(),fetch.unwrap_or(3).min(3),"{sql}");
            if mixed {assert_eq!(rows,vec![vec![SqlValue::Int(3)],vec![SqlValue::Int(2)],vec![SqlValue::Int(1)]][..rows.len()]);}
        }
    }
}

#[test]
fn empty_values_emit_zero_rows_with_the_declared_column_type() {
    let schema=Arc::new(datafusion::common::DFSchema::try_from(arrow::datatypes::Schema::new(vec![Field::new("value",DataType::Int64,true)])).unwrap());
    let plan=LogicalPlan::Values(datafusion::logical_expr::Values{schema,values:vec![]});
    let sql=unparse_plan(plan,SqlDialect::DuckDb).unwrap();
    assert!(DuckDbExecutor::new().run(&[],&sql).unwrap().is_empty(),"{sql}");
}

#[test]
fn top_k_scope_preserves_shadowed_sort_keys_and_outer_offset_limit() {
    for ascending in [false,true] {
        for fetch in [None,Some(2),Some(4)] {
            for offset in [0,1,4] {
                for limit in [None,Some(1),Some(3)] {
                    let input=LogicalPlanBuilder::values(vec![vec![lit(1_i64)],vec![lit(3_i64)],vec![lit(2_i64)]]).unwrap().build().unwrap();
                    let name=input.schema().field(0).name().clone();
                    let plan=LogicalPlanBuilder::from(input).project(vec![col(&name).alias("x")]).unwrap()
                        .project(vec![(lit(100_i64)-col("x")).alias("x")]).unwrap()
                        .sort_with_limit(vec![col("x").sort(ascending,false)],fetch).unwrap()
                        .limit(offset,limit).unwrap().build().unwrap();
                    let values=if ascending {vec![97,98,99]} else {vec![99,98,97]};
                    let expected=values.into_iter().take(fetch.unwrap_or(3)).skip(offset).take(limit.unwrap_or(3))
                        .map(|value|vec![SqlValue::Int(value)]).collect::<Vec<_>>();
                    let sql=unparse_plan(plan,SqlDialect::DuckDb).unwrap();
                    assert_eq!(DuckDbExecutor::new().run(&[],&sql).unwrap(),expected,"{sql}");
                }
            }
        }
    }
}

#[test]
fn sort_scope_keeps_qualified_output_contract_and_values() {
    for qualified in [false,true] {
        let mut input=LogicalPlanBuilder::empty(true).project(vec![lit(7_i64).alias("value")]).unwrap().build().unwrap();
        if qualified {input=LogicalPlanBuilder::from(input).alias("original").unwrap().build().unwrap();}
        let expected=input.schema().clone();
        let scoped=sort_scope(input,"__w_sql_cte_sort_contract".into()).unwrap();
        if qualified { assert_eq!(scoped.schema(),&expected); }
        else {
            // Unqualified consumers resolve by name through the new alias;
            // only pre-existing qualifiers require an exact schema restoration.
            assert_eq!(scoped.schema().fields(),expected.fields());
            assert_eq!(scoped.schema().index_of_column(&Column::from_name("value")).unwrap(),0);
        }
        let sql=unparse_plan(scoped,SqlDialect::DuckDb).unwrap();
        assert_eq!(DuckDbExecutor::new().run(&[],&sql).unwrap(),vec![vec![SqlValue::Int(7)]]);
    }
}

#[test]
fn repeated_top_k_scopes_share_only_immutable_inputs() {
    use datafusion::logical_expr::{create_udf,ColumnarValue,Volatility};
    for volatile in [false,true] {
        let value=if volatile {
            create_udf("scope_volatile_value",vec![],DataType::Int64,Volatility::Volatile,
                Arc::new(|_|Ok(ColumnarValue::Scalar(ScalarValue::Int64(Some(7)))))).call(vec![])
        } else {lit(7_i64)};
        let sorted=LogicalPlanBuilder::empty(true).project(vec![value.alias("value")]).unwrap()
            .sort_with_limit(vec![col("value").sort(true,false)],Some(1)).unwrap().build().unwrap();
        let plan=LogicalPlanBuilder::from(sorted.clone()).union(sorted).unwrap().build().unwrap();
        let fixed=expand_sort_fetch(plan).unwrap();
        let mut aliases=std::collections::BTreeSet::new();
        fixed.apply(|node|{
            if let LogicalPlan::SubqueryAlias(alias)=node {
                if alias.alias.table().starts_with("__w_sql_cte_sort_input_") || alias.alias.table().starts_with("__w_sql_cte_topk_") {
                    aliases.insert(alias.alias.table().to_owned());
                }
            }
            Ok(TreeNodeRecursion::Continue)
        }).unwrap();
        assert_eq!(aliases.len(),if volatile {4}else{2},"{aliases:?}");
    }
}

fn occurrence_filter_input(value: i64, volatile: bool) -> LogicalPlan {
    let schema=Arc::new(arrow::datatypes::Schema::new(vec![arrow::datatypes::Field::new("value",DataType::Int64,false)]));
    let batch=arrow::array::RecordBatch::try_new(schema.clone(),vec![Arc::new(arrow::array::Int64Array::from(vec![value]))]).unwrap();
    let source=datafusion::datasource::MemTable::try_new(schema,vec![vec![batch]]).unwrap();
    let left=LogicalPlanBuilder::scan("source",datafusion::datasource::provider_as_source(Arc::new(source)),None).unwrap()
        .filter(col("value").gt(lit(0_i64))).unwrap()
        .project(vec![col("value"),if volatile {datafusion::functions::math::expr_fn::random().alias("noise")}else{lit(0_f64).alias("noise")}]).unwrap().build().unwrap();
    let right=LogicalPlanBuilder::empty(true).project(vec![lit(1_i64).alias("occurrence")]).unwrap().build().unwrap();
    LogicalPlanBuilder::from(left).cross_join(right).unwrap().build().unwrap()
}
fn occurrence_window(input: LogicalPlan) -> LogicalPlan {
    LogicalPlanBuilder::from(input).window(vec![datafusion::functions_window::expr_fn::row_number()
        .alias("__apply_corr_key_row")]).unwrap().build().unwrap()
}
fn join_filter_scope_names(plan: &LogicalPlan) -> Vec<String> {
    let mut names=Vec::new();
    plan.apply_with_subqueries(|node| {
        if let LogicalPlan::SubqueryAlias(alias)=node {
            if alias.alias.table().starts_with("__w_sql_cte_join_filter_") {names.push(alias.alias.table().to_owned());}
        }
        Ok(TreeNodeRecursion::Continue)
    }).unwrap();
    names
}

#[test]
fn correlation_frontier_filter_scopes_keep_one_occurrence_evaluation() {
    for volatile in [false,true] {
        let frontier=occurrence_window(occurrence_filter_input(1,volatile));
        let combined=LogicalPlanBuilder::from(frontier.clone()).union(frontier).unwrap().build().unwrap();
        let repaired=preserve_join_filter_inputs(combined).unwrap();
        let names=join_filter_scope_names(&repaired);
        assert_eq!(names.len(),2,"{repaired:?}");
        assert_eq!(names[0],names[1],"Consumers of the same frontier must retain one window identity, volatile={volatile}");
    }
}

#[test]
fn correlation_frontier_scopes_keep_distinct_providers_and_independent_volatile_inputs() {
    let left=occurrence_window(occurrence_filter_input(1,false));
    let right=occurrence_window(occurrence_filter_input(2,false));
    assert_eq!(left,right,"Logical equality alone ignores provider values");
    let distinct=LogicalPlanBuilder::from(left).union(right).unwrap().build().unwrap();
    let independent=occurrence_filter_input(1,true);
    let independent=LogicalPlanBuilder::from(independent.clone()).union(independent).unwrap().build().unwrap();
    for original in [distinct,independent] {
        let repaired=preserve_join_filter_inputs(original).unwrap();
        let names=join_filter_scope_names(&repaired);
        assert_eq!(names.len(),2,"{repaired:?}");
        assert_ne!(names[0],names[1],"Different providers and independent volatile inputs cannot become one evaluation");
    }
}

#[test]
fn correlation_frontier_provider_identity_includes_expression_subqueries() {
    use datafusion::logical_expr::Subquery;
    let query=|value| {
        let input=occurrence_filter_input(value,false);
        let subquery=LogicalPlanBuilder::from(input).project(vec![col("value")]).unwrap().limit(0,Some(1)).unwrap().build().unwrap();
        let expression=Expr::ScalarSubquery(Subquery {subquery:Arc::new(subquery),outer_ref_columns:vec![],spans:Default::default()});
        LogicalPlanBuilder::empty(true).project(vec![expression.alias("value")]).unwrap().build().unwrap()
    };
    let left=query(1);let right=query(2);
    assert_eq!(left,right);
    assert!(super::super::shared_scans::same_source_tree(&left,&left.clone()));
    assert!(!super::super::shared_scans::same_source_tree(&left,&right));
}
