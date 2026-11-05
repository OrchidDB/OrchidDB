
    use super::*;

    #[test]
    fn substitution_never_captures_a_constructor_binding() {
        let values = BTreeMap::from([("xs".into(), IrExpr::List(vec![IrExpr::Binding("a".into())]))]);
        let expr = IrExpr::ListTransform {
            list: Box::new(IrExpr::List(vec![IrExpr::Lit(Lit::Int(1))])),
            item: "a".into(),
            map: Box::new(IrExpr::Binding("xs".into())),
        };
        assert!(substitute(&expr, &values).is_none());
    }

    #[test]
    fn selection_does_not_drop_observable_constructor_evaluation() {
        let expr = IrExpr::Call { name: "cypher_subscript".into(), args: vec![
            IrExpr::List(vec![IrExpr::Lit(Lit::Int(1)), IrExpr::Call { name: "rand".into(), args: vec![] }]),
            IrExpr::Lit(Lit::Int(0)),
        ] };
        assert_eq!(select_constructor(expr.clone()), expr);
    }

    #[test]
    fn literal_collection_eligibility_excludes_evaluation_effects() {
        for expression in [
            IrExpr::Call { name: "rand".into(), args: vec![] },
            IrExpr::Binary { op: BinaryOp::Div,
                lhs: Box::new(IrExpr::Lit(Lit::Int(1))),
                rhs: Box::new(IrExpr::Lit(Lit::Int(0))) },
            IrExpr::Binding("outer".into()),
        ] {
            assert!(!literal_tree(&IrExpr::List(vec![IrExpr::Lit(Lit::Int(1)), expression])));
        }
    }

#[test]
fn constructor_selection_obeys_negative_indexes_missing_keys_and_last_duplicate() {
    let list = IrExpr::List(vec![IrExpr::Lit(Lit::Int(3)), IrExpr::binding("outer")]);
    for (index, expected) in [(-3, IrExpr::Lit(Lit::Null)),(-2,IrExpr::Lit(Lit::Int(3))),(-1,IrExpr::binding("outer")),(2,IrExpr::Lit(Lit::Null)),(i64::MIN,IrExpr::Lit(Lit::Null)),(i64::MAX,IrExpr::Lit(Lit::Null))] {
        let call = IrExpr::Call { name:"cypher_subscript".into(), args:vec![list.clone(),IrExpr::Lit(Lit::Int(index))] };
        assert_eq!(select_constructor(call),expected);
    }
    for constructor in ["map","make_map"] {
        let map = IrExpr::Call { name:constructor.into(), args:vec![IrExpr::lit_str("a"),IrExpr::Lit(Lit::Int(1)),IrExpr::lit_str("a"),IrExpr::binding("last")] };
        for name in ["property","cypher_subscript"] {
            for (key,expected) in [("a",IrExpr::binding("last")),("absent",IrExpr::Lit(Lit::Null))] {
                assert_eq!(select_constructor(IrExpr::Call {name:name.into(),args:vec![map.clone(),IrExpr::lit_str(key)]}),expected);
            }
        }
    }
}

#[test]
fn constructor_substitution_rejects_capture_in_every_collection_binder() {
    let values = BTreeMap::from([("xs".into(),IrExpr::List(vec![IrExpr::binding("captured")]))]);
    let source = Box::new(IrExpr::binding("xs"));
    let body = Box::new(IrExpr::binding("xs"));
    let expressions = [
        IrExpr::ListFilter {list:source.clone(),item:"captured".into(),predicate:body.clone()},
        IrExpr::Quantifier {kind:crate::ir::plan::QuantifierKind::All,list:source.clone(),item:"captured".into(),predicate:body.clone()},
        IrExpr::ListReduce {collection:source.clone(),accumulator:"acc".into(),item:"captured".into(),map:body.clone()},
        IrExpr::ListReduce {collection:source,accumulator:"captured".into(),item:"item".into(),map:body},
    ];
    for expression in expressions { assert!(substitute(&expression,&values).is_none(),"{expression:?}"); }
    for expression in [IrExpr::Id("xs".into()),IrExpr::Label("xs".into()),IrExpr::IsBound("xs".into()),IrExpr::SimplePath("xs".into())] {
        assert!(substitute(&expression,&values).is_none());
        assert_eq!(substitute(&expression,&BTreeMap::new()),Some(expression));
    }
}

fn collected_map_input(map: IrExpr, distinct: bool) -> Node {
    Node::GraphProject {
        mode:ProjectMode::ReplaceScope, error_policy:crate::ir::plan::ProjectErrorPolicy::PropagateError,
        items:vec![ProjectionItem {alias:"first".into(),expr:IrExpr::Call {name:"head".into(),args:vec![IrExpr::binding("collected")]}}],
        input:Box::new(Node::GraphAggregate {group:vec![],aggs:vec![crate::ir::expr::AggCall {kind:AggKind::CollectNonNull,alias:"collected".into(),arg:Some(map),distinct}],fields:vec!["collected".into()],input:Node::GraphOneRow.boxed()}),
    }
}

#[test]
fn collected_map_scalarization_preserves_null_carriers_and_rejects_escaping_maps() {
    let map = IrExpr::Call {name:"make_map".into(),args:vec![IrExpr::lit_str("a"),IrExpr::Lit(Lit::Null)]};
    let select = |key| ProjectionItem {alias:"answer".into(),expr:IrExpr::Call {name:"cypher_subscript".into(),args:vec![IrExpr::binding("first"),IrExpr::lit_str(key)]}};
    for key in ["a","absent"] {
        let (outer,inner) = scalarize_collected_map_projection(ProjectMode::ReplaceScope,&[select(key)],&collected_map_input(map.clone(),false)).unwrap();
        assert_eq!(outer[0].expr,IrExpr::binding("first"));
        let Node::GraphProject {input,..} = inner else {panic!()};
        let Node::GraphAggregate {aggs,..} = *input else {panic!()};
        assert_eq!(aggs[0].arg,Some(IrExpr::List(vec![IrExpr::Lit(Lit::Null)])));
    }
    let input = collected_map_input(map.clone(),false);
    for items in [vec![select("a"),select("b")],vec![select("a"),ProjectionItem {alias:"escape".into(),expr:IrExpr::binding("first")}],vec![ProjectionItem {alias:"constant".into(),expr:IrExpr::Lit(Lit::Int(1))}]] {
        assert!(scalarize_collected_map_projection(ProjectMode::ReplaceScope,&items,&input).is_none());
    }
    assert!(scalarize_collected_map_projection(ProjectMode::ReplaceScope,&[select("a")],&collected_map_input(map,true)).is_none());
    let effectful = IrExpr::Call {name:"make_map".into(),args:vec![IrExpr::lit_str("a"),IrExpr::Lit(Lit::Int(1)),IrExpr::lit_str("unused"),IrExpr::Call {name:"rand".into(),args:vec![]}]};
    assert!(scalarize_collected_map_projection(ProjectMode::ReplaceScope,&[select("a")],&collected_map_input(effectful,false)).is_none());
    assert!(scalarize_collected_map_projection(ProjectMode::PreserveVisible,&[select("a")],&input).is_none());
}

fn projection(items: Vec<(&str,IrExpr)>, input: Node) -> Node {
    Node::GraphProject {mode:ProjectMode::ReplaceScope,error_policy:crate::ir::plan::ProjectErrorPolicy::PropagateError,
        items:items.into_iter().map(|(alias,expr)|ProjectionItem{alias:alias.into(),expr}).collect(),input:Box::new(input)}
}
fn project_item(alias:&str,expr:IrExpr)->ProjectionItem {ProjectionItem{alias:alias.into(),expr}}
fn select_index(value:IrExpr,index:i64)->IrExpr {IrExpr::Call{name:"cypher_subscript".into(),args:vec![value,IrExpr::Lit(Lit::Int(index))]}}

#[test]
fn recursive_scalarization_retains_outer_scope_after_inner_collection_disappears() {
    let inner=projection(vec![("xs",IrExpr::List(vec![IrExpr::Lit(Lit::Int(7))]))],Node::GraphOneRow);
    let middle=projection(vec![("v",select_index(IrExpr::binding("xs"),0))],inner);
    let outputs=vec![project_item("answer",IrExpr::binding("v"))];
    let (actual,input)=scalarize_projection(ProjectMode::ReplaceScope,&outputs,&middle).unwrap();
    assert_eq!(actual,outputs);
    assert_eq!(input,projection(vec![("v",IrExpr::Lit(Lit::Int(7)))],Node::GraphOneRow));
    // Repeating the rewrite reaches a fixed point rather than removing v's scope.
    assert!(scalarize_projection(ProjectMode::ReplaceScope,&actual,&input).is_none());
}

#[test]
fn scalarization_preserves_source_dependencies_and_rejects_shadowed_names() {
    let outputs=vec![project_item("answer",select_index(IrExpr::binding("xs"),0))];
    for passthrough in [false,true] {
        let mut items=vec![("xs",IrExpr::List(vec![IrExpr::binding("source")]))];
        if passthrough {items.push(("source",IrExpr::binding("source")));}
        let input=projection(items,Node::GraphOneRow);
        let (actual,remaining)=scalarize_projection(ProjectMode::ReplaceScope,&outputs,&input).unwrap();
        assert_eq!(actual,vec![project_item("answer",IrExpr::binding("source"))]);
        assert_eq!(remaining,projection(vec![("source",IrExpr::binding("source"))],Node::GraphOneRow));
    }
    let shadowed=projection(vec![("xs",IrExpr::List(vec![IrExpr::binding("source")])),("source",IrExpr::Lit(Lit::Int(9)))],Node::GraphOneRow);
    assert!(scalarize_projection(ProjectMode::ReplaceScope,&outputs,&shadowed).is_none());
    let volatile=IrExpr::Call{name:"rand".into(),args:vec![]};
    let input=projection(vec![("xs",IrExpr::List(vec![IrExpr::Lit(Lit::Int(7))])),("sample",volatile.clone())],Node::GraphOneRow);
    let (_,remaining)=scalarize_projection(ProjectMode::ReplaceScope,&outputs,&input).unwrap();
    assert_eq!(remaining,projection(vec![("sample",volatile)],Node::GraphOneRow));
    assert!(scalarize_projection(ProjectMode::PreserveVisible,&outputs,&input).is_none());
    let mut preserve=input.clone();
    if let Node::GraphProject{mode,..}=&mut preserve {*mode=ProjectMode::PreserveVisible;}
    assert!(scalarize_projection(ProjectMode::ReplaceScope,&outputs,&preserve).is_none());
    assert!(scalarize_projection(ProjectMode::ReplaceScope,&outputs,&Node::GraphOneRow).is_none());
}

#[test]
fn substitution_visits_nested_expression_positions_without_rebinding_locals() {
    let values=BTreeMap::from([("xs".into(),IrExpr::List(vec![IrExpr::Lit(Lit::Int(7))]))]);
    let shapes=|leaf:IrExpr|vec![
        IrExpr::Not(Box::new(leaf.clone())),IrExpr::IsNull(Box::new(leaf.clone())),IrExpr::IsNotNull(Box::new(leaf.clone())),
        IrExpr::Binary{op:BinaryOp::Eq,lhs:Box::new(leaf.clone()),rhs:Box::new(leaf.clone())},
        IrExpr::StringPredicate{op:crate::ir::expr::StringOp::Contains,target:Box::new(leaf.clone()),pattern:Box::new(leaf.clone())},
        IrExpr::Case{arms:vec![(leaf.clone(),leaf.clone())],otherwise:Some(Box::new(leaf.clone()))},
        IrExpr::List(vec![leaf.clone()]),
        IrExpr::ListTransform{list:Box::new(leaf.clone()),item:"item".into(),map:Box::new(leaf.clone())},
        IrExpr::ListFilter{list:Box::new(leaf.clone()),item:"item".into(),predicate:Box::new(leaf.clone())},
        IrExpr::Quantifier{kind:crate::ir::plan::QuantifierKind::Any,list:Box::new(leaf.clone()),item:"item".into(),predicate:Box::new(leaf.clone())},
        IrExpr::ListReduce{collection:Box::new(leaf.clone()),item:"item".into(),accumulator:"acc".into(),map:Box::new(leaf)},
    ];
    for (input,expected) in shapes(IrExpr::binding("xs")).into_iter().zip(shapes(values["xs"].clone())) {
        assert_eq!(substitute(&input,&values),Some(expected));
    }
    let local=IrExpr::ListTransform{list:Box::new(IrExpr::binding("xs")),item:"xs".into(),map:Box::new(IrExpr::binding("xs"))};
    let expected=IrExpr::ListTransform{list:Box::new(values["xs"].clone()),item:"xs".into(),map:Box::new(IrExpr::binding("xs"))};
    assert_eq!(substitute(&local,&values),Some(expected));
    let label=IrExpr::HasLabel{binding:"xs".into(),label:"Person".into()};
    assert!(substitute(&label,&values).is_none());
    assert_eq!(substitute(&label,&BTreeMap::new()),Some(label));
}
