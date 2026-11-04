
    use super::*;
    #[test]
    fn non_nan_facts_require_implementation_identity_and_preserve_unknown_domains() {
        let plan = datafusion::logical_expr::LogicalPlanBuilder::empty(true).build().unwrap();
        let helper = super::super::language_functions::cypher_to_float(lit("NaN"));
        assert!(excludes_nan(&plan, &helper, 64));
        assert!(excludes_nan(&plan, &lit(f64::INFINITY), 64));
        assert!(!excludes_nan(&plan, &lit(f64::NAN), 64));
        // Finite operands can produce NaN: never propagate blindly through arithmetic.
        assert!(!excludes_nan(&plan, &(helper.clone() / helper), 64));
        let spoof = datafusion::logical_expr::create_udf(
            super::super::language_functions::CYPHER_TO_FLOAT,
            vec![DataType::Utf8], DataType::Float64,
            datafusion::logical_expr::Volatility::Immutable,
            Arc::new(|_| panic!("domain analysis must not execute a UDF")),
        ).call(vec![lit("NaN")]);
        assert!(!excludes_nan(&plan, &spoof, 64));
    }
    #[test]
    fn conditional_domains_require_every_result_and_numeric_cast_inputs() {
        let plan = datafusion::logical_expr::LogicalPlanBuilder::empty(true).build().unwrap();
        let helper = super::super::language_functions::cypher_to_float(lit("12"));
        let conditional = datafusion::logical_expr::when(lit(true), helper.clone())
            .otherwise(lit(ScalarValue::Float64(None))).unwrap();
        assert!(excludes_nan(&plan, &conditional, 64));
        let possible_nan = datafusion::logical_expr::when(lit(true), helper)
            .otherwise(lit(f64::NAN)).unwrap();
        assert!(!excludes_nan(&plan, &possible_nan, 64));
        assert!(excludes_nan(&plan, &Expr::Cast(Cast::new(Box::new(lit(12_i64)), DataType::Float64)), 64));
        assert!(!excludes_nan(&plan, &Expr::Cast(Cast::new(Box::new(lit("NaN")), DataType::Float64)), 64));
    }
    #[test]
    fn finite_branch_guards_require_repeatability_and_matching_searched_case() {
        let plan = datafusion::logical_expr::LogicalPlanBuilder::empty(true).build().unwrap();
        let parsed = Expr::TryCast(TryCast::new(Box::new(lit("NaN")), DataType::Float64));
        let bounds = parsed.clone().gt(lit(f64::NEG_INFINITY))
            .and(parsed.clone().lt(lit(f64::INFINITY)));
        let null = lit(ScalarValue::Float64(None));
        let guarded = datafusion::logical_expr::when(bounds.clone(), parsed.clone())
            .otherwise(null.clone()).unwrap();
        assert!(excludes_nan(&plan, &guarded, 64));
        let expansion = super::super::scalar_semantics::conformant_string_to_float(lit("NaN")).unwrap();
        assert!(excludes_nan(&plan, &expansion, 64));
        for unsafe_case in [
            datafusion::logical_expr::when(bounds.clone().or(lit(true)), parsed.clone()).otherwise(null.clone()).unwrap(),
            datafusion::logical_expr::when(bounds.clone(), lit(f64::NAN)).otherwise(null.clone()).unwrap(),
            Expr::Case(Case::new(Some(Box::new(lit(false))), vec![(Box::new(bounds), Box::new(parsed))], Some(Box::new(null.clone())))),
        ] {
            assert!(!excludes_nan(&plan, &unsafe_case, 64));
        }
        let volatile = datafusion::logical_expr::create_udf("changing_float", vec![], DataType::Float64,
            datafusion::logical_expr::Volatility::Volatile,
            Arc::new(|_| panic!("proof must not execute functions"))).call(vec![]);
        let guarded = datafusion::logical_expr::when(
            volatile.clone().gt(lit(f64::NEG_INFINITY)).and(volatile.clone().lt(lit(f64::INFINITY))), volatile)
            .otherwise(null).unwrap();
        assert!(!excludes_nan(&plan, &guarded, 64));
    }
    #[tokio::test]
    async fn comparison_resolves_each_conversion_operand_once() {
        crate::ir::diagnostics::without_constant_folding(async {
            let graph = crate::language::cypher::preparation::prepare(
                "RETURN toFloat($left) > toFloat($right) AS result",
                &BTreeMap::from([("left".into(), Value::String("12".into())),
                    ("right".into(), Value::String("5".into()))]), None,
            ).unwrap();
            let mut options = RelBackendOptions::default();
            options.rule_context.trace_limit = 128;
            let lowered = RelBackend::with_options(options).lower(&graph, &PropertyGraph::new()).unwrap();
            let selections = lowered.rule_context.trace().into_iter()
                .filter(|t| t.rule_id == "semantic.cypher.string_to_float" && t.outcome == "changed").count();
            assert_eq!(selections, 2, "each operand must select its implementation once");
        }).await;
    }
    #[tokio::test]
    async fn required_scalar_rules_run_without_optional_optimization() {
        crate::ir::diagnostics::without_constant_folding(async {
            for (query, rule) in [
                ("UNWIND [1,2] AS x RETURN x = 2", "scalar.comparison.cypher"),
                ("UNWIND [1,2] AS x RETURN x IN [2, null]", "scalar.collection.membership"),
                (
                    "RETURN date({year:1816,month:1,day:1}) AS d",
                    "scalar.temporal.typed",
                ),
            ] {
                let graph =
                    crate::language::cypher::preparation::prepare(query, &Default::default(), None)
                        .unwrap();
                let mut options = RelBackendOptions::default();
                options.rule_context.optimize = false;
                options.rule_context.trace_limit = 128;
                let lowered = RelBackend::with_options(options)
                    .lower(&graph, &PropertyGraph::new())
                    .unwrap();
                assert!(
                    lowered
                        .rule_context
                        .trace()
                        .iter()
                        .any(|t| t.rule_id == rule && t.outcome == "changed"),
                    "{query}: {:?}",
                    lowered.rule_context.trace()
                );
            }
        })
        .await;
    }
