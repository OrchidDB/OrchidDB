use super::*;
#[test]
fn range_provider_retains_exact_boundaries_for_sql_lowering() {
    for (start, stop, step) in [
        (i64::MAX - 1, i64::MAX, 1),
        (i64::MIN + 1, i64::MIN, -1),
        (i64::MAX, i64::MIN, i64::MIN),
    ] {
        let provider = IntegerRange::new(start, stop, step, "n".into());
        assert_eq!(
            (provider.start, provider.stop, provider.step),
            (start, stop, step)
        );
        assert_eq!(provider.schema().field(0).data_type(), &DataType::Int64);
    }
}

#[tokio::test]
async fn outer_empty_ranges_preserve_parent_occurrences_and_replace_the_range_binding() {
    let graph = PropertyGraph::new();
    for (start, stop, step) in [(3, 1, 1), (1, 3, -1)] {
        let mut context = LoweringContext {
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
        let input = context
            .lower_node(&Node::GraphValues {
                bindings: vec!["parent".into(), "item".into()],
                rows: vec![vec![Value::Int(7), Value::Int(9)]; 2],
                bulk: None,
            })
            .unwrap();
        let args = [start, stop, step].map(|value| IrExpr::Lit(Lit::Int(value)));
        let scan_count = context.scan_counter;
        let lowered = context
            .lower_range_unwind(input, &args, "item", true)
            .unwrap();
        assert_eq!(
            context.scan_counter, scan_count,
            "empty ranges need no physical table function"
        );
        let plan = LogicalPlanBuilder::from(lowered.plan)
            .project(vec![col_exact("parent"), col_exact("item")])
            .unwrap()
            .build()
            .unwrap();
        let batches = super::super::execute_test_plan(plan).await.unwrap();
        assert_eq!(batches.iter().map(RecordBatch::num_rows).sum::<usize>(), 2);
        for batch in batches {
            assert_eq!(batch.column(1).null_count(), batch.num_rows());
            for row in 0..batch.num_rows() {
                assert_eq!(
                    arrow::util::display::array_value_to_string(batch.column(0), row).unwrap(),
                    "7"
                );
            }
        }
    }
}

#[test]
fn literal_range_bounds_reject_dynamic_and_invalid_arguments_without_coercion() {
    for (args, expected) in [
        (vec![], "arity"),
        (
            vec![IrExpr::binding("start"), IrExpr::Lit(Lit::Int(1))],
            "start",
        ),
        (
            vec![IrExpr::Lit(Lit::Int(1)), IrExpr::binding("stop")],
            "stop",
        ),
        (
            vec![
                IrExpr::Lit(Lit::Int(1)),
                IrExpr::Lit(Lit::Int(3)),
                IrExpr::Lit(Lit::Float(1.5)),
            ],
            "step",
        ),
        (
            vec![
                IrExpr::Lit(Lit::Int(1)),
                IrExpr::Lit(Lit::Int(3)),
                IrExpr::Lit(Lit::Int(0)),
            ],
            "zero",
        ),
    ] {
        assert!(
            literal_range_bounds(&args)
                .unwrap_err()
                .to_string()
                .contains(expected)
        );
    }
    assert_eq!(
        literal_range_bounds(&[
            IrExpr::Lit(Lit::Int(i64::MIN)),
            IrExpr::Lit(Lit::Int(i64::MAX))
        ])
        .unwrap(),
        (i64::MIN, i64::MAX, 1)
    );
    assert_eq!(
        IntegerRange::new(1, 3, 1, "item".into()).table_type(),
        TableType::Temporary
    );
}
