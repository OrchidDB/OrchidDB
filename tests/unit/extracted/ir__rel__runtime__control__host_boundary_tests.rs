
    use super::*;
    #[derive(Debug)]
    struct FrontierRunner { fail: bool }
    impl SubplanRunner for FrontierRunner {
        fn run(&self, _plan: &LogicalPlan, state: &mut KernelState) -> IrResult<Vec<Row>> {
            assert_eq!(state.batch_key.as_deref(), Some("correlation"));
            state.context.side_effects.insert("visited".into(), Value::Bool(true));
            if self.fail { return Err(RuntimeError::Runtime("host failure".into())); }
            Ok(state.frontier.clone())
        }
    }
    fn body() -> Subplan {
        Subplan {
            plan: kernel("TestFrontier", vec![], |_, state| Ok(state.frontier.clone())),
            prepared: Default::default(), observable: false, batchable: false,
            batch_names: Default::default(), barrier: false, group_barrier: false,
            group_split: None, writer_split: None,
        }
    }
    #[test]
    fn injected_runner_keeps_empty_frontier_distinct_and_restores_state_on_error() {
        let graph = PropertyGraph::new();
        for fail in [false, true] {
            let mut state = KernelState::for_host(graph.clone(), Arc::new(FrontierRunner { fail }), &Default::default());
            for frontier in [vec![], vec![Row::new()]] {
                let expected = frontier.len();
                let result = body().run_internal(frontier, &graph, &mut state.context, Some("correlation".into()));
                if fail { assert_eq!(result.unwrap_err().to_string(), "host failure"); }
                else { assert_eq!(result.unwrap().len(), expected); }
                assert!(matches!(state.context.side_effects["visited"], Value::Bool(true)));
                assert!(state.context.subplan_runner.is_some());
            }
        }
    }
    #[derive(Debug)]
    struct ApplyRunner { copies: usize, corrupt_identity: u8 }
    impl SubplanRunner for ApplyRunner {
        fn run(&self, _plan: &LogicalPlan, state: &mut KernelState) -> IrResult<Vec<Row>> {
            let mut rows=Vec::new();
            for row in &state.frontier {
                assert_eq!(row.bulk,1,"each correlated occurrence starts with unit bulk");
                for _ in 0..self.copies {
                    let mut row=row.clone().with("v",Value::Int(7));row.bulk=2;
                    if let Some(key)=&state.batch_key {
                        if self.corrupt_identity==1 {row.bindings.remove(key);}
                        if self.corrupt_identity==2 {row.bindings.insert(key.clone(),Value::UInt64(u64::MAX));}
                    }
                    rows.push(row);
                }
            }
            Ok(rows)
        }
    }

    #[test]
    fn correlated_apply_keeps_duplicate_occurrences_and_parent_multiplicity() {
        let graph=PropertyGraph::new();
        for batched in [false,true] {
            for copies in [0,1,2] {
                for kind in [ApplyKind::Inner,ApplyKind::Optional,ApplyKind::Semi,ApplyKind::Anti] {
                    let mut outer=vec![Row::new().with("id",Value::Int(1));2];
                    outer[0].bulk=2;outer[1].bulk=3;
                    let mut state=KernelState::for_host(graph.clone(),Arc::new(ApplyRunner{copies,corrupt_identity:0}),&Default::default());
                    let mut right=body();right.batchable=batched;
                    let rows=apply_op(kind,&["id".into()],&["v".into(),"missing".into()],OptionalMissing::Null,
                        outer,&right,&graph,&mut state.context).unwrap();
                    let expected=match kind {
                        ApplyKind::Inner=>2*copies,ApplyKind::Optional=>2*copies.max(1),
                        ApplyKind::Semi=>if copies>0{2}else{0},ApplyKind::Anti=>if copies==0{2}else{0},_=>unreachable!(),
                    };
                    assert_eq!(rows.len(),expected,"{kind:?}, batch={batched}, copies={copies}");
                    for (index,row) in rows.iter().enumerate() {
                        let group=if matches!(kind,ApplyKind::Inner|ApplyKind::Optional) {index/copies.max(1)} else {index};
                        let multiplier=if copies>0 && matches!(kind,ApplyKind::Inner|ApplyKind::Optional){2}else{1};
                        assert_eq!(row.bulk,[2,3][group]*multiplier);
                        assert_eq!(row.bindings["id"],Value::Int(1));
                        assert!(!row.bindings.keys().any(|key|key.starts_with("\0orchiddb_apply_")));
                        if kind==ApplyKind::Optional {
                            assert_eq!(row.bindings["missing"],Value::Null);
                            assert_eq!(row.bindings["v"],if copies==0{Value::Null}else{Value::Int(7)});
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn correlated_apply_rejects_lost_occurrence_identity_and_bulk_overflow() {
        let graph=PropertyGraph::new();
        for corruption in [1,2] {
            let mut state=KernelState::for_host(graph.clone(),Arc::new(ApplyRunner{copies:1,corrupt_identity:corruption}),&Default::default());
            let mut right=body();right.batchable=true;
            let error=apply_op(ApplyKind::Inner,&[],&["v".into()],OptionalMissing::Null,
                vec![Row::new(),Row::new()],&right,&graph,&mut state.context).unwrap_err().to_string();
            assert!(error.contains("occurrence identity"),"{error}");
        }
        for kind in [ApplyKind::Inner,ApplyKind::Optional] {
            let mut state=KernelState::for_host(graph.clone(),Arc::new(ApplyRunner{copies:1,corrupt_identity:0}),&Default::default());
            let mut outer=Row::new();outer.bulk=u64::MAX;
            let error=apply_op(kind,&[],&["v".into()],OptionalMissing::Null,vec![outer],&body(),&graph,&mut state.context).unwrap_err().to_string();
            assert!(error.contains("bulk overflow"),"{error}");
        }
    }

    #[test]
    fn scalar_apply_preserves_empty_results_and_rejects_multiple_rows() {
        let graph=PropertyGraph::new();
        for copies in [0,1,2] {
            let mut state=KernelState::for_host(graph.clone(),Arc::new(ApplyRunner{copies,corrupt_identity:0}),&Default::default());
            let result=apply_op(ApplyKind::Scalar,&[],&["v".into(),"missing".into()],OptionalMissing::Null,
                vec![Row::new().with("outer",Value::Int(9))],&body(),&graph,&mut state.context);
            if copies>1 {assert!(result.unwrap_err().to_string().contains("more than one row"));continue;}
            let rows=result.unwrap();assert_eq!(rows.len(),1);assert_eq!(rows[0].bulk,1);
            assert_eq!(rows[0].bindings["outer"],Value::Int(9));
            assert_eq!(rows[0].bindings["v"],if copies==0{Value::Null}else{Value::Int(7)});
            assert_eq!(rows[0].bindings["missing"],Value::Null);
        }
    }


#[derive(Debug)]
struct BindingRunner { shared: Value }
impl SubplanRunner for BindingRunner {
    fn run(&self, _plan: &LogicalPlan, state: &mut KernelState) -> IrResult<Vec<Row>> {
        Ok(state.frontier.iter().map(|row| {
            let mut row = row.clone().with("shared", self.shared.clone())
                .with("__scratch", Value::Int(99)).with("value", Value::Int(7));
            for name in ["__cypher_match_history_a", "__cypher_exists_history_a", "__cypher_pattern_history_a"] {
                row.bindings.insert(name.into(), Value::List(vec![Value::Int(2)]));
            }
            row
        }).collect())
    }
}

#[test]
fn cypher_apply_reconciles_shared_values_without_conflating_internal_histories() {
    let graph = PropertyGraph::new();
    for batched in [false, true] {
        for (outer_value, inner_value, matches) in [
            (Value::Int(1),Value::Long(1),true),
            (Value::Int(1),Value::Int(2),false),
            // Shared imported nulls agree; this is binding compatibility, not a WHERE predicate.
            (Value::Null,Value::Null,true),
            (Value::Null,Value::Int(1),false),
            (Value::String("same".into()),Value::String("same".into()),true),
        ] {
            let mut outer = Row::new().with("shared",outer_value).with("__scratch",Value::Int(1));
            for name in ["__cypher_match_history_a", "__cypher_exists_history_a", "__cypher_pattern_history_a"] {
                outer.bindings.insert(name.into(),Value::List(vec![Value::Int(1)]));
            }
            let mut state = KernelState::for_host(graph.clone(),Arc::new(BindingRunner {shared:inner_value}),&Default::default());
            let mut right = body();right.batchable = batched;
            let rows = apply_op(ApplyKind::Inner,&["shared".into()],&["value".into()],OptionalMissing::Null,
                vec![outer.clone(),outer],&right,&graph,&mut state.context).unwrap();
            assert_eq!(rows.len(),if matches {2} else {0});
            for row in rows {
                assert_eq!(row.bindings["__scratch"],Value::Int(1));
                assert_eq!(row.bindings["value"],Value::Int(7));
                for name in ["__cypher_match_history_a", "__cypher_exists_history_a", "__cypher_pattern_history_a"] {
                    assert_eq!(row.bindings[name],Value::List(vec![Value::Int(2)]));
                }
            }
        }
    }
}

#[test]
fn empty_scalar_apply_without_output_bindings_reports_its_contract_error() {
    let graph = PropertyGraph::new();
    let mut state = KernelState::for_host(graph.clone(),Arc::new(ApplyRunner {copies:0,corrupt_identity:0}),&Default::default());
    let error = apply_op(ApplyKind::Scalar,&[],&[],OptionalMissing::Null,vec![Row::new()],&body(),&graph,&mut state.context).unwrap_err();
    assert!(error.to_string().contains("scalar apply produced no output bindings"));
}
