
    use super::*;

    #[test]
    fn required_graph_implementation_stages_cannot_be_skipped() {
        let graph = crate::language::cypher::preparation::prepare(
            "RETURN 1 AS n", &BTreeMap::new(), None,
        ).unwrap();
        for (budget, expected) in [
            (0, "graph.procedure.catalog_binding"),
            (1, "graph.path.required_representation"),
        ] {
            let mut context = RuleContext::default();
            context.optimize = false;
            context.budget.stage_visits = budget;
            let result = prepare_graph_implementations(graph.clone(), &PropertyGraph::new(), &context);
            assert!(matches!(result, Err(RelError::RulePipeline(ref error))
                if error.to_string().contains(expected)), "required stage {expected} bypassed: {result:?}");
        }
        let mut context = RuleContext::default();
        context.optimize = false;
        let prepared = prepare_graph_implementations(graph.clone(), &PropertyGraph::new(), &context).unwrap();
        assert_eq!(prepared, graph);
        assert_eq!(context.visits(), 2);
    }
