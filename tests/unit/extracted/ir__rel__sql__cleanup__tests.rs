
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

