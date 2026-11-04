
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
