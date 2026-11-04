
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

