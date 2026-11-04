
    use super::*;

    #[test]
    fn singleton_unwind_obeys_optional_rule_policy() {
        let graph = crate::language::cypher::preparation::prepare(
            "UNWIND [1] AS x RETURN x", &BTreeMap::new(), None,
        ).unwrap();
        for optimize in [false, true] {
            let mut options = RelBackendOptions::default();
            options.rule_context.optimize = optimize;
            options.rule_context.trace_limit = 128;
            let lowered = RelBackend::with_options(options).lower(&graph, &PropertyGraph::new()).unwrap();
            let selected = lowered.rule_context.trace().iter().any(|trace|
                trace.rule_id == "rel.collection.singleton_unwind" && trace.outcome == "changed");
            assert_eq!(selected, optimize);
        }
    }

    #[tokio::test]
    async fn nested_unwind_preserves_required_rule_failure() {
        crate::ir::diagnostics::without_constant_folding(async {
            // The required scalar rule runs below two UNWIND operators. Its
            // exhausted budget must remain fatal to SQL-island selection.
            let graph = crate::language::cypher::preparation::prepare(
                "WITH toFloat($raw) + toFloat($raw) AS n UNWIND [1] AS x UNWIND [2] AS y RETURN n, x, y",
                &BTreeMap::from([("raw".into(), Value::String("12".into()))]),
                None,
            ).unwrap();
            let mut options = RelBackendOptions::default();
            options.rule_context.optimize = false;
            // Allow catalog binding and required path normalization. The
            // independent SemanticResolve budget then fails on the third
            // temporal candidate inside WITH, below both UNWIND operators.
            options.rule_context.budget.stage_visits = 6;
            let result = RelBackend::with_options(options).lower(&graph, &PropertyGraph::new());
            match result {
                Err(RelError::RulePipeline(error)) => {
                    assert!(error.to_string().contains("exhausted its rewrite budget"));
                    assert!(error.to_string().contains("scalar.temporal"));
                }
                Err(other) => panic!("required failure became a fallback miss: {other}"),
                Ok(_) => panic!("exhausted required stage unexpectedly succeeded"),
            }
        }).await;
    }
    fn integer(value: ScalarValue) -> Option<i64> {
        match value {
            ScalarValue::Int32(v) => v.map(i64::from), ScalarValue::Int64(v) => v,
            other => panic!("expected integer, got {other:?}"),
        }
    }

    fn context(graph: &PropertyGraph, native_values: bool) -> LoweringContext<'_> {
        LoweringContext {
            island_memo: None, graph,
            options: RelBackendOptions { native_values, ..Default::default() },
            policy: crate::ir::policy::GraphPlanPolicy::cypher(), language: Language::Cypher,
            scan_counter: 0, correlate_plan: None, rdf_typed_terms_used: false,
            scan_logical_labels: false, gremlin_label_binds: Default::default(),
            unordered_collects: Default::default(), unordered_distincts: Default::default(),
        }
    }

    #[test]
    fn relational_list_operators_reject_scalar_domains_with_specific_errors() {
        let graph=PropertyGraph::new();
        for value in [Value::Int(7),Value::Bool(true),Value::String("scalar".into())] {
            let input=Node::GraphValues {bindings:vec!["value".into()],rows:vec![vec![value]],bulk:None};
            let mut context=context(&graph,false);
            let lowered=context.lower_node(&input).unwrap();
            let error=context.lower_native_list(&lowered.plan,&IrExpr::binding("value")).unwrap_err();
            assert!(error.to_string().contains("list operation over non-list type"),"{error}");
            let error=context.lower_unwind_dynamic(lowered,&IrExpr::binding("value"),"item",false).unwrap_err();
            assert!(error.to_string().contains("GraphUnwind over non-constant list expression"),"{error}");
            for kind in [QuantifierKind::All,QuantifierKind::Any,QuantifierKind::None,QuantifierKind::Single] {
                let error=context.lower_quantifier(kind,"item",&IrExpr::binding("value"),
                    &IrExpr::Lit(Lit::Bool(true)),"answer",&input).unwrap_err();
                assert!(error.to_string().contains("GraphQuantifier over non-list expression"),"{error}");
            }
        }
    }

    #[tokio::test]
    async fn explicit_collection_order_preserves_duplicates_and_null_positions() {
        let graph = PropertyGraph::new();
        for native in [false, true] {
            for descending in [false, true] {
                for nulls_first in [false, true] {
                    let mut context = context(&graph, native);
                    let input = Node::GraphValues { bindings: vec!["v".into()],
                        rows: [Value::Int(3),Value::Null,Value::Int(1),Value::Int(3)].into_iter().map(|v|vec![v]).collect(), bulk: None };
                    let order = SortKey { expr: IrExpr::binding("v"),
                        dir: if descending {SortDir::Desc} else {SortDir::Asc},
                        nulls: if nulls_first {NullsOrder::First} else {NullsOrder::Last} };
                    let result = context.lower_collect(&IrExpr::binding("v"),false,&[order],"values",&input).unwrap();
                    let batches = super::super::execute_test_plan(result.plan).await.unwrap();
                    assert_eq!(batches[0].num_rows(),1);
                    let value = ScalarValue::try_from_array(batches[0].column(0),0).unwrap();
                    let ScalarValue::List(values) = value else { panic!("expected list"); };
                    let actual = values.value(0);
                    let actual = (0..actual.len()).map(|i|integer(ScalarValue::try_from_array(&actual,i).unwrap())).collect::<Vec<_>>();
                    let mut expected = if descending {vec![Some(3),Some(3),Some(1)]} else {vec![Some(1),Some(3),Some(3)]};
                    if nulls_first {expected.insert(0,None);} else {expected.push(None);}
                    assert_eq!(actual,expected);
                }
            }
        }
    }

    #[test]
    fn collection_rejects_distinct_without_order_and_incomplete_correlation() {
        let graph = PropertyGraph::new();
        let mut context = context(&graph,false);
        let error = context.lower_collect(&IrExpr::binding("v"),true,&[],"values",&Node::GraphOneRow).unwrap_err();
        assert!(error.to_string().contains("stable first-occurrence ordering"));
        let input = Node::GraphValues { bindings: vec!["__apply_corr_key_0".into(),"v".into()],
            rows: vec![vec![Value::Int(1),Value::Int(2)]],bulk:None };
        let error = context.lower_collect(&IrExpr::binding("v"),false,&[],"values",&input).unwrap_err();
        assert!(error.to_string().contains("no correlated outer input"));
        context.correlate_plan = Some(LogicalPlanBuilder::empty(true).build().unwrap());
        let error = context.lower_collect(&IrExpr::binding("v"),false,&[],"values",&input).unwrap_err();
        assert!(error.to_string().contains("unavailable from the outer input"));
    }

    #[tokio::test]
    async fn opaque_quantifier_domains_distinguish_scalars_nulls_and_lists() {
        let graph = PropertyGraph::new();
        for kind in [QuantifierKind::All,QuantifierKind::Any,QuantifierKind::None,QuantifierKind::Single] {
            for predicate in [Lit::Bool(true),Lit::Bool(false),Lit::Null] {
                let values = [Value::List(vec![Value::Int(1),Value::Int(2)]),Value::Null,Value::List(vec![]),Value::Int(7)];
                let input = Node::GraphValues { bindings: vec!["id".into(),"xs".into()],
                    rows: values.into_iter().enumerate().map(|(id,v)|vec![Value::Int(id as i64),v]).collect(),bulk:None };
                let mut context = context(&graph,true);
                let result = context.lower_quantifier(kind,"x",&IrExpr::binding("xs"),&IrExpr::Lit(predicate.clone()),"answer",&input).unwrap();
                let plan = LogicalPlanBuilder::from(result.plan).project(vec![col_exact("id"),col_exact("answer")]).unwrap()
                    .sort(vec![col_exact("id").sort(true,false)]).unwrap().build().unwrap();
                let batches = super::super::execute_test_plan(plan).await.unwrap();
                let actual = batches.iter().flat_map(|b|(0..b.num_rows()).map(|i|ScalarValue::try_from_array(b.column(1),i).unwrap())).collect::<Vec<_>>();
                let nonempty = match (&predicate,kind) {
                    (Lit::Null,_)=>None,
                    (Lit::Bool(value),QuantifierKind::All|QuantifierKind::Any)=>Some(*value),
                    (Lit::Bool(value),QuantifierKind::None)=>Some(!value),
                    (Lit::Bool(_),QuantifierKind::Single)=>Some(false),_=>unreachable!(),
                };
                let empty = Some(matches!(kind,QuantifierKind::All|QuantifierKind::None));
                assert_eq!(actual,[nonempty,None,empty,None].into_iter().map(ScalarValue::Boolean).collect::<Vec<_>>(),"{kind:?}, {predicate:?}");
            }
        }
    }

    #[tokio::test]
    async fn outer_unwind_preserves_empty_and_null_parent_occurrences() {
        let graph = PropertyGraph::new();
        for native in [false,true] {
            for outer in [false,true] {
                let values = [Value::List(vec![Value::Int(1),Value::Int(2)]),Value::Null,
                    Value::List(vec![]),Value::List(vec![Value::Null]),Value::List(vec![Value::Int(3),Value::Null])];
                let input = Node::GraphValues { bindings: vec!["id".into(),"xs".into()],
                    rows: values.into_iter().enumerate().map(|(id,v)|vec![Value::Int(id as i64),v]).collect(), bulk:None };
                let mut context = context(&graph,native);
                let input = context.lower_node(&input).unwrap();
                let lowered = context.lower_unwind_dynamic(input,&IrExpr::binding("xs"),"x",outer).unwrap();
                let plan = LogicalPlanBuilder::from(lowered.plan).project(vec![col_exact("id"),col_exact("x")]).unwrap()
                    .sort(vec![col_exact("id").sort(true,false),col_exact("x").sort(true,false)]).unwrap().build().unwrap();
                let batches = super::super::execute_test_plan(plan).await.unwrap();
                let actual = batches.iter().flat_map(|b|(0..b.num_rows()).map(|i|(
                    integer(ScalarValue::try_from_array(b.column(0),i).unwrap()),integer(ScalarValue::try_from_array(b.column(1),i).unwrap())))).collect::<Vec<_>>();
                let mut expected=vec![(0,Some(1)),(0,Some(2))];
                if outer {expected.extend([(1,None),(2,None)]);}
                expected.extend([(3,None),(4,Some(3)),(4,None)]);
                assert_eq!(actual,expected.into_iter().map(|(id,x)|(Some(id),x)).collect::<Vec<_>>(),"native={native}, outer={outer}");
            }
        }
    }

