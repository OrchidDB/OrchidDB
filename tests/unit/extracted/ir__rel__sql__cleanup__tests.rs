
    use super::*;
    use datafusion::prelude::col;

    #[test]
    fn folded_quantifiers_emit_one_select() {
        let plan = LogicalPlanBuilder::empty(true)
            .project(vec![lit(0_i64).alias("__w_one_row")]).unwrap()
            .project(vec![col("__w_one_row"), lit(true).alias("__cypher_quantifier_0")]).unwrap()
            .project(vec![col("__cypher_quantifier_0").alias("result")]).unwrap()
            .build().unwrap();
        assert_eq!(super::super::unparse_plan(plan, SqlDialect::DuckDb).unwrap(),
            "SELECT true AS \"result\"");
    }

    #[test]
    fn keeps_zero_row_source() {
        let plan = LogicalPlanBuilder::empty(false)
            .project(vec![lit(true).alias("temporary")]).unwrap()
            .project(vec![col("temporary").alias("result")]).unwrap()
            .build().unwrap();
        let plan = simplify(plan).unwrap();
        let LogicalPlan::Projection(p) = plan else { panic!("projection expected") };
        assert!(matches!(p.input.as_ref(), LogicalPlan::EmptyRelation(e) if !e.produce_one_row));
    }

    #[test]
    fn pure_output_rename_retains_runtime_computation_once() {
        let input = occurrences();
        let computed = LogicalPlanBuilder::from(input.clone())
            .project(vec![(col("value") + lit(1_i64)).alias("temporary")]).unwrap()
            .project(vec![col("temporary").alias("result")]).unwrap().build().unwrap();
        let schema = computed.schema().clone();
        let simplified = simplify(computed).unwrap();
        assert_eq!(simplified.schema(), &schema);
        let LogicalPlan::Projection(p) = simplified else { panic!("projection expected") };
        assert_eq!(p.input.as_ref(), &input);
        assert_eq!(p.expr, vec![(col("value") + lit(1_i64)).alias("result")]);
    }

    #[test]
    fn scalar_subquery_rename_keeps_correlated_input_projection() {
        let input = LogicalPlanBuilder::from(occurrences())
            .project(vec![col("value").alias("renamed")]).unwrap().build().unwrap();
        let reference = Expr::OuterReferenceColumn(input.schema().field(0).clone(), Column::new_unqualified("renamed"));
        let subquery = Expr::ScalarSubquery(datafusion::logical_expr::Subquery {
            subquery: Arc::new(LogicalPlanBuilder::empty(true)
                .project(vec![reference.clone().alias("value")]).unwrap().build().unwrap()),
            outer_ref_columns: vec![reference], spans: Default::default(),
        });
        let plan = LogicalPlanBuilder::from(input.clone())
            .project(vec![subquery.clone().alias("temporary")]).unwrap()
            .project(vec![col("temporary").alias("renamed")]).unwrap().build().unwrap();
        let LogicalPlan::Projection(p) = simplify(plan).unwrap() else { panic!("projection expected") };
        assert_eq!(p.input.as_ref(), &input);
        assert_eq!(p.expr, vec![subquery.alias("renamed")]);
    }

    #[test]
    fn closed_scalar_subquery_does_not_need_a_literal_seed() {
        let subquery = Expr::ScalarSubquery(datafusion::logical_expr::Subquery {
            subquery: Arc::new(LogicalPlanBuilder::empty(true)
                .project(vec![lit(7_i64).alias("value")]).unwrap().build().unwrap()),
            outer_ref_columns: vec![], spans: Default::default(),
        });
        let seed = LogicalPlanBuilder::empty(true).project(vec![lit(0_i64).alias("seed")]).unwrap().build().unwrap();
        let plan = LogicalPlanBuilder::from(seed).project(vec![subquery.clone().alias("result")]).unwrap().build().unwrap();
        let LogicalPlan::Projection(p) = simplify(plan).unwrap() else { panic!("projection expected") };
        assert!(matches!(p.input.as_ref(), LogicalPlan::EmptyRelation(e) if e.produce_one_row && e.schema.fields().is_empty()));
        assert_eq!(p.expr, vec![subquery.alias("result")]);
    }

    #[test]
    fn set_comparison_keeps_its_correlated_literal_seed_and_scope() {
        use datafusion::logical_expr::{expr::{SetComparison, SetQuantifier}, Subquery};
        let seed = LogicalPlanBuilder::empty(true)
            .project(vec![lit(7_i64).alias("seed")]).unwrap().build().unwrap();
        let reference = Expr::OuterReferenceColumn(seed.schema().field(0).clone(), Column::new_unqualified("seed"));
        let subquery = Subquery {
            subquery: Arc::new(LogicalPlanBuilder::empty(true)
                .project(vec![reference.clone().alias("value")]).unwrap().build().unwrap()),
            outer_ref_columns: vec![reference], spans: Default::default(),
        };
        let comparison = Expr::SetComparison(SetComparison::new(Box::new(lit(7_i64)), subquery,
            datafusion::logical_expr::Operator::Eq, SetQuantifier::Any));
        assert!(has_subquery(&comparison).unwrap());
        let input = LogicalPlanBuilder::from(seed.clone()).project(vec![comparison.alias("temporary")]).unwrap().build().unwrap();
        let plan = LogicalPlanBuilder::from(input).project(vec![col("temporary").alias("result")]).unwrap().build().unwrap();
        let simplified = simplify(plan).unwrap();
        let LogicalPlan::Projection(outer) = simplified else { panic!("projection expected") };
        let LogicalPlan::Projection(inner) = outer.input.as_ref() else { panic!("set-comparison scope must remain") };
        assert_eq!(inner.input.as_ref(), &seed);
        assert!(matches!(inner.expr[0], Expr::Alias(ref alias) if matches!(alias.expr.as_ref(), Expr::SetComparison(_))));
    }

    #[test]
    fn does_not_duplicate_or_discard_computations() {
        // A division can throw, even though it is immutable. Neither a double
        // reference nor an unused result justifies removing this boundary.
        for exprs in [vec![col("computed").alias("a"), col("computed").alias("b")],
                      vec![lit(true).alias("result")]] {
            let plan = LogicalPlanBuilder::empty(true)
                .project(vec![(lit(1_i64) / lit(0_i64)).alias("computed")]).unwrap()
                .project(exprs).unwrap().build().unwrap();
            let original = plan.clone();
            assert_eq!(simplify(plan).unwrap(), original);
        }
    }

    #[test]
    fn does_not_duplicate_volatile_functions() {
        let random = datafusion::logical_expr::create_udf("volatile_value", vec![], DataType::Int64,
            datafusion::logical_expr::Volatility::Volatile,
            Arc::new(|_| Ok(datafusion::logical_expr::ColumnarValue::Scalar(ScalarValue::Int64(Some(1))))));
        let plan = LogicalPlanBuilder::empty(true)
            .project(vec![random.call(vec![]).alias("value")]).unwrap()
            .project(vec![col("value").alias("a"), col("value").alias("b")]).unwrap()
            .build().unwrap();
        assert_eq!(simplify(plan.clone()).unwrap(), plan);
    }
    fn singleton(expr: Expr, one_row: bool) -> LogicalPlan {
        LogicalPlanBuilder::empty(one_row).project(vec![expr.alias("seed")]).unwrap().build().unwrap()
    }
    fn occurrences() -> LogicalPlan {
        let row = |value| LogicalPlanBuilder::empty(true)
            .project(vec![lit(value).alias("value")]).unwrap().build().unwrap();
        LogicalPlanBuilder::from(row(ScalarValue::Int64(Some(1))))
            .union(row(ScalarValue::Int64(Some(1)))).unwrap()
            .union(row(ScalarValue::Int64(None))).unwrap().build().unwrap()
    }
    fn cross(left: LogicalPlan, right: LogicalPlan) -> LogicalPlan {
        LogicalPlanBuilder::from(left).cross_join(right).unwrap().build().unwrap()
    }

    #[test]
    fn singleton_cross_join_preserves_schema_constants_and_input() {
        for seed_on_left in [true,false] {
            let seed = singleton(lit(ScalarValue::Int64(None)), true);
            let rows = occurrences();
            let plan = if seed_on_left { cross(seed,rows.clone()) } else { cross(rows.clone(),seed) };
            let schema = plan.schema().clone();
            let simplified = simplify(plan).unwrap();
            assert_eq!(simplified.schema(), &schema);
            let LogicalPlan::Projection(p) = simplified else { panic!("expected projection") };
            assert_eq!(p.input.as_ref(), &rows);
            let index = if seed_on_left {0} else {1};
            assert_eq!(simple_value(&p.expr[index]), Some(lit(ScalarValue::Int64(None))));
        }
    }

    #[test]
    fn zero_row_computed_and_conditional_join_inputs_are_not_singleton_seeds() {
        let volatile = datafusion::logical_expr::create_udf("volatile_seed", vec![], DataType::Int64,
            datafusion::logical_expr::Volatility::Volatile,
            Arc::new(|_| Ok(datafusion::logical_expr::ColumnarValue::Scalar(ScalarValue::Int64(Some(1))))));
        let subquery = Expr::ScalarSubquery(datafusion::logical_expr::Subquery {
            subquery: Arc::new(singleton(lit(2_i64),true)), outer_ref_columns: vec![], spans: Default::default(),
        });
        for seed in [singleton(lit(0_i64),false), singleton(lit(1_i64)/lit(0_i64),true),
                     singleton(volatile.call(vec![]),true), singleton(subquery,true)] {
            let plan = cross(seed,occurrences());
            assert_eq!(simplify(plan.clone()).unwrap(), plan);
        }
        let plan = cross(singleton(lit(0_i64),true),occurrences());
        let LogicalPlan::Join(mut join) = plan else { panic!("join expected") };
        join.filter = Some(lit(true));
        let plan = LogicalPlan::Join(join);
        assert_eq!(simplify(plan.clone()).unwrap(),plan);
    }

    #[cfg(feature = "duckdb")]
    #[test]
    fn singleton_join_sql_keeps_duplicates_nulls_and_empty_results() {
        fn rows(plan: LogicalPlan) -> Vec<Vec<ScalarValue>> {
            let sql = super::super::unparse_plan(plan, SqlDialect::DuckDb).unwrap();
            assert!(!sql.contains("CROSS JOIN"), "{sql}");
            let connection = duckdb::Connection::open_in_memory().unwrap();
            let mut statement = connection.prepare(&sql).unwrap();
            let batches: Vec<RecordBatch> = statement.query_arrow([]).unwrap().collect();
            batches.iter().flat_map(|batch| (0..batch.num_rows()).map(|row|
                batch.columns().iter().map(|array| ScalarValue::try_from_array(array,row).unwrap()).collect::<Vec<_>>()
            )).collect()
        }
        for left in [true,false] {
            let seed = singleton(lit(7_i64),true);
            let other = occurrences();
            let plan = if left { cross(seed,other) } else { cross(other,seed) };
            let actual = rows(plan);
            // This test runs raw SQL, without execute_prepared's output-schema
            // normalization. DuckDB infers INTEGER for these small literals.
            let seven = ScalarValue::Int32(Some(7));
            let one = ScalarValue::Int32(Some(1));
            let null = ScalarValue::Int32(None);
            let expected = if left { vec![vec![seven.clone(),one.clone()],vec![seven.clone(),one],vec![seven,null]] }
                else { vec![vec![one.clone(),seven.clone()],vec![one,seven.clone()],vec![null,seven]] };
            assert_eq!(actual,expected);
        }
        let empty = LogicalPlanBuilder::from(occurrences()).filter(lit(false)).unwrap().build().unwrap();
        assert!(rows(cross(singleton(lit(7_i64),true),empty)).is_empty());
    }


fn scope_subquery_expressions(reference: Option<Expr>) -> Vec<Expr> {
    use datafusion::logical_expr::{Subquery, expr::{Exists, InSubquery, SetComparison, SetQuantifier}, Operator};
    let subquery = Subquery {
        subquery: Arc::new(LogicalPlanBuilder::empty(true)
            .project(vec![reference.clone().unwrap_or_else(|| lit(7_i64)).alias("inner_value")]).unwrap().build().unwrap()),
        outer_ref_columns: reference.into_iter().collect(), spans: Default::default(),
    };
    vec![Expr::ScalarSubquery(subquery.clone()), Expr::Exists(Exists::new(subquery.clone(), false)),
        Expr::InSubquery(InSubquery::new(Box::new(lit(1_i64)), subquery.clone(), false)),
        Expr::SetComparison(SetComparison::new(Box::new(lit(1_i64)), subquery, Operator::Eq, SetQuantifier::Any))]
}

#[tokio::test]
async fn correlation_scopes_avoid_reserved_aliases_and_preserve_each_subquery_kind() {
    let input = LogicalPlanBuilder::from(occurrences()).alias("__w_sql_cte_correlation_0").unwrap()
        .alias("__w_sql_cte_correlation_1").unwrap()
        .project(vec![col("value").alias("outer_value")]).unwrap().build().unwrap();
    let reference = Expr::OuterReferenceColumn(input.schema().field(0).clone(), Column::new_unqualified("outer_value"));
    for (kind, expression) in scope_subquery_expressions(Some(reference)).into_iter().enumerate() {
        let original = LogicalPlanBuilder::from(input.clone())
            .project(vec![col("outer_value"), expression.alias("result")]).unwrap().build().unwrap();
        let scoped = super::super::correlation::preserve_scopes(original.clone()).unwrap();
        assert_eq!(scoped.schema(), original.schema());
        let LogicalPlan::Projection(projection) = &scoped else { panic!() };
        let LogicalPlan::SubqueryAlias(alias) = projection.input.as_ref() else { panic!("missing correlation scope") };
        assert_eq!(alias.alias.table(), "__w_sql_cte_correlation_2");
        assert!(matches!(&projection.expr[0], Expr::Column(column) if column.relation.as_ref().unwrap().table() == alias.alias.table()));
        assert_eq!(super::super::correlation::preserve_scopes(scoped.clone()).unwrap(), scoped);
        let batches = crate::ir::rel::execute_test_plan(scoped).await.unwrap_or_else(|error| panic!("subquery kind {kind}: {error}"));
        let mut actual = Vec::new();
        for batch in batches { for row in 0..batch.num_rows() {
            actual.push(batch.columns().iter().map(|array| if array.is_null(row) { "NULL".to_string() }
                else { arrow::util::display::array_value_to_string(array, row).unwrap() }).collect::<Vec<_>>());
        }}
        actual.sort();
        let expected = if kind == 0 { vec![vec!["1","1"],vec!["1","1"],vec!["NULL","NULL"]] }
            else if kind == 1 { vec![vec!["1","true"],vec!["1","true"],vec!["NULL","true"]] }
            else { vec![vec!["1","true"],vec!["1","true"],vec!["NULL","NULL"]] };
        assert_eq!(actual, expected, "subquery kind {kind}");
    }
}

#[test]
fn correlation_scopes_leave_closed_subqueries_and_free_outer_inputs_unchanged() {
    let input = singleton(lit(7_i64), true);
    for expression in scope_subquery_expressions(None) {
        let plan = LogicalPlanBuilder::from(input.clone()).project(vec![expression.alias("result")]).unwrap().build().unwrap();
        assert_eq!(super::super::correlation::preserve_scopes(plan.clone()).unwrap(), plan);
    }
    let outer = Expr::OuterReferenceColumn(Arc::new(Field::new("parent", DataType::Int64, true)), Column::new_unqualified("parent"));
    let input = LogicalPlanBuilder::empty(true).project(vec![outer.alias("outer_value")]).unwrap().build().unwrap();
    let reference = Expr::OuterReferenceColumn(input.schema().field(0).clone(), Column::new_unqualified("outer_value"));
    for expression in scope_subquery_expressions(Some(reference)) {
        let plan = LogicalPlanBuilder::from(input.clone()).project(vec![expression.alias("result")]).unwrap().build().unwrap();
        assert_eq!(super::super::correlation::preserve_scopes(plan.clone()).unwrap(), plan);
    }
}

#[tokio::test]
async fn in_subquery_nullability_includes_both_operands_and_survives_optimization() {
    use datafusion::logical_expr::{Subquery, expr::InSubquery, ExprSchemable};
    for left in [Some(7_i64), Some(8), None] {
        for right in [Some(7_i64), None] {
            for empty in [false, true] {
                for negated in [false, true] {
                    let subquery = Subquery { subquery: Arc::new(LogicalPlanBuilder::empty(!empty)
                        .project(vec![lit(ScalarValue::Int64(right)).alias("right_value")]).unwrap().build().unwrap()),
                        outer_ref_columns: vec![], spans: Default::default() };
                    let expression = Expr::InSubquery(InSubquery::new(Box::new(lit(ScalarValue::Int64(left))), subquery, negated));
                    let seed = LogicalPlanBuilder::empty(true).build().unwrap();
                    assert_eq!(expression.nullable(seed.schema()).unwrap(), left.is_none() || right.is_none());
                    let plan = LogicalPlanBuilder::from(seed).project(vec![expression.clone().alias("result"),
                        expression.is_null().alias("missing")]).unwrap().build().unwrap();
                    let batches = crate::ir::rel::execute_test_plan(plan).await.unwrap();
                    assert_eq!(batches.iter().map(|b| b.num_rows()).sum::<usize>(), 1);
                    let batch = batches.iter().find(|b| b.num_rows() != 0).unwrap();
                    let expected = if empty { Some(negated) } else { left.zip(right).map(|(a,b)| (a == b) != negated) };
                    assert_eq!(ScalarValue::try_from_array(batch.column(0), 0).unwrap(), ScalarValue::Boolean(expected),
                        "left {left:?}, right {right:?}, empty {empty}, negated {negated}");
                    assert_eq!(ScalarValue::try_from_array(batch.column(1), 0).unwrap(), ScalarValue::Boolean(Some(expected.is_none())));
                }
            }
        }
    }
}
