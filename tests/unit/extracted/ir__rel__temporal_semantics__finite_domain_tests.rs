
use super::*;

#[test]
fn finite_domains_include_null_extension_and_survive_limits() {
    let input = |name: &str, value: i64| {
        LogicalPlanBuilder::empty(true)
            .project(vec![lit(value).alias(name)])
            .unwrap()
            .build()
            .unwrap()
    };
    for kind in [
        JoinType::Inner,
        JoinType::Left,
        JoinType::Right,
        JoinType::Full,
    ] {
        let joined = LogicalPlanBuilder::from(input("left_value", 1))
            .join(
                input("right_value", 2),
                kind,
                (Vec::<Column>::new(), Vec::<Column>::new()),
                Some(lit(true)),
            )
            .unwrap()
            .limit(0, Some(1))
            .unwrap()
            .build()
            .unwrap();
        for (column, value, nullable) in [
            (
                "left_value",
                1,
                matches!(kind, JoinType::Right | JoinType::Full),
            ),
            (
                "right_value",
                2,
                matches!(kind, JoinType::Left | JoinType::Full),
            ),
        ] {
            let mut expected = vec![ScalarValue::Int64(Some(value))];
            if nullable {
                expected.push(ScalarValue::Int64(None));
            }
            assert_eq!(finite_values(&joined, column), Some(expected));
        }
        assert_eq!(finite_values(&joined, "missing"), None);
    }
}

#[tokio::test]
async fn finite_temporal_arithmetic_order_keys_preserve_ties_and_nulls() {
    let graph = PropertyGraph::new();
    let context = LoweringContext {
        island_memo: None,
        graph: &graph,
        options: RelBackendOptions::default(),
        policy: crate::ir::policy::GraphPlanPolicy::cypher(),
        language: Language::Cypher,
        scan_counter: 0,
        correlate_plan: None,
        rdf_typed_terms_used: false,
        scan_logical_labels: false,
        gremlin_label_binds: Default::default(),
        unordered_collects: Default::default(),
        unordered_distincts: Default::default(),
    };
    let date = |text: &str| {
        crate::ir::temporal::transport::encoded_scalar(
            &crate::ir::temporal::parse("date", text).unwrap(),
        )
        .unwrap()
    };
    let null = ScalarValue::try_from(&date("2020-01-01").data_type()).unwrap();
    for only_null in [false, true] {
        let values = if only_null {
            vec![null.clone()]
        } else {
            vec![
                date("2020-01-03"),
                date("2020-01-01"),
                date("2020-01-03"),
                null.clone(),
            ]
        };
        let mut inputs = values.into_iter().map(|value| {
            LogicalPlanBuilder::empty(true)
                .project(vec![lit(value).alias("date")])
                .unwrap()
                .build()
                .unwrap()
        });
        let mut input = inputs.next().unwrap();
        for next in inputs {
            input = LogicalPlanBuilder::from(input)
                .union(next)
                .unwrap()
                .build()
                .unwrap();
        }
        let expression = IrExpr::Binary {
            op: crate::ir::expr::BinaryOp::Add,
            lhs: Box::new(IrExpr::binding("date")),
            rhs: Box::new(IrExpr::Call {
                name: "cypher_temporal.duration".into(),
                args: vec![IrExpr::Lit(Lit::String("P1D".into()))],
            }),
        };
        let key = finite_order_key(&context, &input, &expression).expect("finite temporal rank");
        if !only_null {
            let Expr::Case(case) = &key else {
                panic!("partial evaluation should retain one CASE")
            };
            assert!(case.when_then_expr.iter().all(|(_, value)| matches!(
                value.as_ref(),
                Expr::Literal(ScalarValue::Int64(_), _)
            )));
        }
        let plan = LogicalPlanBuilder::from(input)
            .project(vec![key.alias("rank")])
            .unwrap()
            .build()
            .unwrap();
        let batches = crate::ir::rel::execute_test_plan(plan).await.unwrap();
        let actual = batches
            .iter()
            .flat_map(|batch| {
                (0..batch.num_rows())
                    .map(|row| ScalarValue::try_from_array(batch.column(0), row).unwrap())
            })
            .collect::<Vec<_>>();
        let expected = if only_null {
            vec![ScalarValue::Int64(None)]
        } else {
            vec![Some(1), Some(0), Some(1), None]
                .into_iter()
                .map(ScalarValue::Int64)
                .collect()
        };
        assert_eq!(actual, expected);
    }
}

#[test]
fn outer_unnest_empty_or_null_lists_include_null_in_domain() {
    let empty = ScalarValue::List(ScalarValue::new_list(&[], &DataType::Int64, true));
    let null = ScalarValue::try_from(&empty.data_type()).unwrap();
    for source in [empty, null] {
        let input = LogicalPlanBuilder::empty(true)
            .project(vec![lit(source).alias("step")])
            .unwrap()
            .build()
            .unwrap();
        let mut options = datafusion::common::UnnestOptions::default();
        options.preserve_nulls = true;
        let plan = LogicalPlanBuilder::from(input)
            .unnest_column_with_options(Column::new_unqualified("step"), options)
            .unwrap()
            .build()
            .unwrap();
        assert_eq!(
            finite_values(&plan, "step"),
            Some(vec![ScalarValue::Int64(None)])
        );
    }
}
