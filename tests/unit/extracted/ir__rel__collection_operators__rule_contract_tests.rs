
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


#[tokio::test]
async fn cypher_collection_display_preserves_dynamic_null_boolean_and_numeric_items() {
    let graph = PropertyGraph::new();
    let mut ctx = context(&graph, false);
    let input = Node::GraphValues {
        bindings: vec!["flag".into(), "number".into(), "text".into(), "missing".into()],
        rows: vec![
            vec![Value::Bool(true), Value::Float(1.5), Value::String("雪".into()), Value::Null],
            vec![Value::Bool(false), Value::Float(-2.25), Value::String("".into()), Value::Null],
            vec![Value::Null, Value::Null, Value::Null, Value::Null],
        ], bulk: None,
    };
    let plan = ctx.lower_node(&input).unwrap().plan;
    let expr = ctx.lower_collection_expr(&plan, &[
        IrExpr::binding("flag"), IrExpr::binding("number"),
        IrExpr::binding("text"), IrExpr::binding("missing"),
    ]).unwrap();
    let projected = LogicalPlanBuilder::from(plan).project(vec![expr.alias("display")]).unwrap().build().unwrap();
    let batches = super::super::execute_test_plan(projected).await.unwrap();
    let actual: Vec<_> = batches.iter().flat_map(|batch| (0..batch.num_rows()).map(|i|
        ScalarValue::try_from_array(batch.column(0), i).unwrap())).collect();
    assert_eq!(actual, ["[True,1.500000,雪,]", "[False,-2.250000,,]", "[,,,]"].map(|s| ScalarValue::Utf8(Some(s.into()))));
}

#[test]
fn cypher_dynamic_maps_reject_duplicate_keys_malformed_pairs_and_dynamic_names() {
    let graph = PropertyGraph::new();
    let mut ctx = context(&graph, false);
    let plan = ctx.lower_node(&Node::GraphValues {
        bindings: vec!["value".into()], rows: vec![vec![Value::Int(3)]], bulk: None,
    }).unwrap().plan;
    let key = IrExpr::Lit(Lit::String("k".into()));
    let value = IrExpr::binding("value");
    for (args, expected) in [
        (vec![IrExpr::List(vec![key.clone(),key.clone()]), IrExpr::List(vec![value.clone(),value.clone()])], "duplicate map key"),
        (vec![IrExpr::List(vec![key.clone()]), IrExpr::List(vec![value.clone(),value.clone()])], "length mismatch"),
        (vec![key.clone()], "map arity"),
        (vec![value.clone(),value.clone()], "dynamic map key"),
    ] {
        let error = ctx.lower_cypher_map(&plan, &args).unwrap_err().to_string();
        assert!(error.contains(expected), "{error}");
    }
    for (args, expected) in [
        (vec![key], "make_map arity"),
        (vec![value.clone(),value], "dynamic make_map key"),
    ] {
        let error = ctx.lower_make_map(&plan, &args).unwrap_err().to_string();
        assert!(error.contains(expected), "{error}");
    }
}

#[tokio::test]
async fn cypher_map_display_keeps_dynamic_field_order() {
    let graph = PropertyGraph::new();
    let mut ctx = context(&graph, false);
    let plan = ctx.lower_node(&Node::GraphValues {
        bindings: vec!["value".into()], rows: vec![vec![Value::Int(3)]], bulk: None,
    }).unwrap().plan;
    let keys = vec![IrExpr::Lit(Lit::String("z".into())), IrExpr::Lit(Lit::String("a".into()))];
    let values = vec![IrExpr::binding("value"), IrExpr::Lit(Lit::Int(8))];
    let list_map = ctx.lower_cypher_map(&plan, &[IrExpr::List(keys.clone()), IrExpr::List(values.clone())]).unwrap();
    let pairs = vec![keys[0].clone(),values[0].clone(),keys[1].clone(),values[1].clone()];
    let tagged = ctx.lower_cypher_map(&plan, &pairs).unwrap();
    let debug = ctx.lower_make_map(&plan, &pairs).unwrap();
    let projected = LogicalPlanBuilder::from(plan).project(vec![list_map.alias("plain"),tagged.alias("tagged"),debug.alias("debug")]).unwrap().build().unwrap();
    let batches = super::super::execute_test_plan(projected).await.unwrap();
    for (i, expected) in ["{z=3, a=8}", "m[{\"z\":\"3\",\"a\":\"8\"}]", "Map({\"z\": String(\"3\"), \"a\": String(\"8\")})"].iter().enumerate() {
        assert_eq!(ScalarValue::try_from_array(batches[0].column(i),0).unwrap(), ScalarValue::Utf8(Some((*expected).into())));
    }
}

#[tokio::test]
async fn literal_collection_insert_preserves_order_duplicates_and_nulls() {
    let graph = PropertyGraph::new();
    let mut ctx = context(&graph, false);
    let plan = ctx.lower_node(&Node::GraphValues {
        bindings: vec!["value".into()], rows: vec![vec![Value::Long(4)], vec![Value::Null]], bulk: None,
    }).unwrap().plan;
    let list = IrExpr::List(vec![IrExpr::binding("value"), IrExpr::Lit(Lit::Null)]);
    let mut expressions = vec![];
    for (i, name) in ["list_append", "array_append", "array_push_back", "list_prepend", "array_prepend", "array_push_front"].iter().enumerate() {
        expressions.push(ctx.lower_constant_collection_function(&plan, name, &[list.clone(), IrExpr::Lit(Lit::Int(4))]).unwrap().alias(format!("v{i}")));
    }
    let projected = LogicalPlanBuilder::from(plan).project(expressions).unwrap().build().unwrap();
    let batches = super::super::execute_test_plan(projected).await.unwrap();
    for batch in batches {
        for column in 0..6 {
            for row in 0..batch.num_rows() {
                let ScalarValue::List(array) = ScalarValue::try_from_array(batch.column(column),row).unwrap() else {panic!("expected typed list")};
                let items = array.value(0);
                let actual = (0..items.len()).map(|i| integer(ScalarValue::try_from_array(&items,i).unwrap())).collect::<Vec<_>>();
                let value = if row == 0 { Some(4) } else { None };
                let expected = if column < 3 { vec![value,None,Some(4)] } else { vec![Some(4),value,None] };
                assert_eq!(actual,expected,"column={column}, row={row}");
            }
        }
    }
}

#[test]
fn dynamic_collection_dispatch_declines_bad_arity_and_unknown_functions() {
    let graph = PropertyGraph::new();
    let mut ctx = context(&graph, false);
    let plan = ctx.lower_node(&Node::GraphOneRow).unwrap().plan;
    for name in ["list_append", "array_append", "array_push_back", "list_prepend", "array_prepend", "array_push_front", "list_extract", "list_element", "element_at", "list_unique", "not_a_collection_function"] {
        assert!(ctx.lower_dynamic_collection_function(&plan,name,&[]).unwrap().is_none(),"{name}");
    }
    let error=ctx.lower_constant_collection_function(&plan,"not_a_collection_function",&[IrExpr::binding("missing")]).unwrap_err().to_string();
    assert!(error.contains("not relationally lowered yet"),"{error}");
}
