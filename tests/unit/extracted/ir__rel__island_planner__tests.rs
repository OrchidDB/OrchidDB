
    use super::*;
    #[test]
    fn correlation_is_only_closed_by_its_binding_operator() {
        use crate::ir::plan::{ApplyKind, EmitMode, PathObjects};
        let correlate = || Node::GraphCorrelate {
            bindings: vec!["current".into()],
        };
        let apply = Node::GraphApply {
            kind: ApplyKind::Semi,
            correlation: vec!["current".into()],
            outputs: vec![],
            optional_missing: crate::ir::policy::OptionalMissing::Null,
            left: Node::GraphOneRow.boxed(),
            right: correlate().boxed(),
        };
        let memo = IslandMemo::new(&apply);
        assert!(memo.lock().unwrap().safe(&apply));
        let Node::GraphApply { right, .. } = &apply else {
            unreachable!()
        };
        assert!(!memo.lock().unwrap().safe(right));
        let mut repeat = Node::GraphRepeat {
            loop_name: None,
            times: Some(8),
            emit: EmitMode::AfterLoop,
            until_first: false,
            until: None,
            until_traversal: None,
            path: None,
            path_objects: PathObjects::VerticesOnly,
            prefix_predicate: None,
            prefix_traversal: None,
            seed: Node::GraphOneRow.boxed(),
            body: correlate().boxed(),
        };
        assert!(IslandMemo::new(&repeat).lock().unwrap().safe(&repeat));
        // A nested repeat's seed still reads the enclosing row. It must not
        // be cached or scheduled as a standalone island.
        let Node::GraphRepeat { seed, .. } = &mut repeat else {
            unreachable!()
        };
        *seed = correlate().boxed();
        assert!(!IslandMemo::new(&repeat).lock().unwrap().safe(&repeat));
    }

    #[tokio::test]
    async fn failed_parent_reuses_child_without_freezing_later_queries() {
        let root = Node::GraphProject {
            mode: crate::ir::plan::ProjectMode::PreserveVisible,
            error_policy: crate::ir::plan::ProjectErrorPolicy::PropagateError,
            items: vec![crate::ir::plan::ProjectionItem {
                alias: "list".into(),
                expr: IrExpr::List(vec![IrExpr::lit_int(1)]),
            }],
            input: Box::new(Node::GraphNodeScan {
                binding: "current".into(),
                labels: LabelExpr::Any,
                graph: "g".into(),
            }),
        };
        let graph = PropertyGraph::new();
        graph.insert_node("person", BTreeMap::new());
        let backend = RelBackend::new().preserving_traverser_state();
        let policy = GraphPlanPolicy::gremlin();
        let memo = IslandMemo::new(&root);
        assert!(
            backend
                .lower_island(&policy, &root, &graph, memo.clone())
                .is_err()
        );
        let Node::GraphProject { input, .. } = &root else {
            unreachable!()
        };
        let first = backend
            .lower_island(&policy, input, &graph, memo.clone())
            .unwrap();
        assert!(memo.lock().unwrap().hits > 0);
        graph.insert_node("person", BTreeMap::new());
        let fresh = backend
            .lower_island(&policy, input, &graph, IslandMemo::new(&root))
            .unwrap();
        let old = sql::plan_tables(&first.plan).await.unwrap();
        let new = sql::plan_tables(&fresh.plan).await.unwrap();
        assert_eq!(
            old.iter()
                .flat_map(|t| &t.batches)
                .map(|b| b.num_rows())
                .sum::<usize>(),
            1
        );
        assert_eq!(
            new.iter()
                .flat_map(|t| &t.batches)
                .map(|b| b.num_rows())
                .sum::<usize>(),
            2
        );
    }
