use super::*;

#[test]
fn parameter_property_filter_retains_binding_and_parameter_identity() {
    let mut lowerer = Lowerer::new();
    lowerer.replace_scope(vec!["outside".to_string()]);
    let result = apply_property_filters(
        &mut lowerer,
        Node::GraphOneRow,
        "person",
        &Expr::Parameter("criteria".into()),
        &BTreeSet::new(),
    )
    .unwrap();
    let Node::GraphFilter { condition, input } = result else {
        panic!("missing filter")
    };
    assert_eq!(
        condition,
        IrExpr::Call {
            name: "cypher_properties_match".into(),
            args: vec![
                IrExpr::Binding("person".into()),
                IrExpr::Call {
                    name: "parameter".into(),
                    args: vec![IrExpr::Lit(Lit::String("criteria".into()))]
                }
            ]
        }
    );
    assert!(matches!(*input, Node::GraphOneRow));
    assert_eq!(lowerer.visible_set(), BTreeSet::from(["outside".into()]));
}

#[test]
fn pattern_filter_errors_preserve_the_callers_scope() {
    for recursive in [false, true] {
        for expression in [
            Expr::Variable("missing".into()),
            Expr::Map(vec![("weight".into(), Expr::Variable("missing".into()))]),
        ] {
            let mut lowerer = Lowerer::new();
            let scope = BTreeSet::from(["outside".to_string()]);
            lowerer.replace_scope(scope.iter().cloned());
            let result = if recursive {
                apply_variable_length_property_filters(
                    &mut lowerer,
                    Node::GraphOneRow,
                    "path",
                    "source",
                    &expression,
                    &scope,
                )
            } else {
                apply_property_filters(
                    &mut lowerer,
                    Node::GraphOneRow,
                    "person",
                    &expression,
                    &scope,
                )
            };
            assert!(result.is_err(), "unbound property expression must fail");
            assert!(result.unwrap_err().to_string().contains("missing"));
            assert_eq!(lowerer.visible_set(), scope);
        }
    }
}

#[test]
fn recursive_parameter_filter_quantifies_over_edges_without_leaking_its_local() {
    let mut lowerer = Lowerer::new();
    let scope = BTreeSet::from(["path".to_string(), "source".to_string()]);
    lowerer.replace_scope(scope.iter().cloned());
    let result = apply_variable_length_property_filters(
        &mut lowerer,
        Node::GraphOneRow,
        "path",
        "source",
        &Expr::Parameter("criteria".into()),
        &scope,
    )
    .unwrap();
    assert!(matches!(&result, Node::GraphFilter { .. }));
    let printed = format!("{result:?}");
    assert!(printed.contains("cypher_properties_match"), "{printed}");
    assert!(printed.contains("criteria"), "{printed}");
    assert!(printed.contains("path"), "{printed}");
    assert_eq!(lowerer.visible_set(), scope);
}

#[test]
fn expansion_retains_empty_range_input_and_selects_relationship_reuse_policy() {
    use crate::language::cypher::{parser::parse_query,ast::Clause};
    for (range, variable, empty) in [("",false,false),("*0..2",true,false),("*3..1",true,true)] {
        let query=parse_query(&format!("MATCH (a)-[r:R{range}]->(b:Person) RETURN b")).unwrap();
        let Clause::Match(clause)=&query.clauses[0] else {panic!()};
        let chain=&clause.patterns[0].element.chains[0];
        for target_exists in [false,true] {
            for repeatable in [false,true] {
                let result=lower_expand(Node::GraphCorrelate{bindings:vec!["a".into()]},
                    "a","b",target_exists,&chain.relationship,Some("r"),&chain.node,
                    Some("p".into()),Some("history".into()),repeatable);
                if empty {
                    let filter=if target_exists {result} else {
                        let Node::GraphJoin{left,right,..}=result else {panic!("new endpoint must retain its schema")};
                        assert!(matches!(*right,Node::GraphNodeScan{binding,..} if binding=="b"));
                        *left
                    };
                    let Node::GraphFilter{condition,input}=filter else {panic!("empty range must filter input")};
                    assert_eq!(condition,IrExpr::Lit(Lit::Bool(false)));
                    assert!(matches!(*input,Node::GraphCorrelate{bindings} if bindings==vec!["a"]));
                } else {
                    let Node::GraphExpand{match_mode,target_mode,path,history,..}=result else {panic!()};
                    assert_eq!(match_mode,if repeatable&&variable {MatchMode::RepeatableElements} else {MatchMode::DifferentRelationships});
                    assert_eq!(target_mode,if target_exists {TargetMode::Existing} else {TargetMode::BindNew});
                    assert_eq!(path,Some("p".into()));
                    assert_eq!(history,Some("history".into()));
                }
            }
        }
    }
}

fn parsed_part(pattern: &str) -> PatternPart {
    use crate::language::cypher::{parser::parse_query, ast::Clause};
    let query = parse_query(&format!("MATCH {pattern} RETURN 1")).unwrap();
    let Clause::Match(clause) = &query.clauses[0] else { panic!("expected MATCH") };
    clause.patterns[0].clone()
}

#[test]
fn pattern_binding_conflicts_leave_outer_scope_and_kinds_intact() {
    for (pattern, name, kind, message) in [
        ("(x)", "x", BindingKind::Int, "node pattern"),
        ("(a)-[r]->(x)", "x", BindingKind::Relationship, "node pattern"),
        ("(a)-[x]->(b)", "x", BindingKind::Node, "expected"),
        ("(a)-[x*1..2]->(b)", "x", BindingKind::Relationship, "expected"),
        ("(a)-[x]->(b)", "x", BindingKind::RecursiveRelationship, "expected"),
        ("x = (a)-[r]->(b)", "x", BindingKind::Path, "VariableAlreadyBound"),
        ("x = (x)-[r]->(b)", "outside", BindingKind::Int, "VariableAlreadyBound"),
        ("x = (a)-[x]->(b)", "outside", BindingKind::Int, "VariableAlreadyBound"),
        ("x = (a)-[r]->(x)", "outside", BindingKind::Int, "VariableAlreadyBound"),
    ] {
        for optional in [false, true] {
            let mut lowerer = Lowerer::new();
            lowerer.add_visible_kind(name, kind);
            let before = lowerer.visible_set();
            let error = lower_pattern_part(&mut lowerer, Node::GraphOneRow,
                &parsed_part(pattern), optional, None, false).unwrap_err();
            assert!(error.to_string().contains(message), "{pattern}: {error}");
            assert_eq!(lowerer.visible_set(), before, "{pattern}");
            assert_eq!(lowerer.binding_kind(name), Some(kind));
        }
    }
}

#[test]
fn optional_pattern_outputs_are_typed_and_only_new_bindings_become_nullable() {
    for optional in [false, true] {
        for (range, expected_rel) in [("", BindingKind::Relationship), ("*0..2", BindingKind::RecursiveRelationship)] {
            let mut lowerer = Lowerer::new();
            lowerer.add_visible_kind("a", BindingKind::Node);
            lowerer.add_visible_kind("outside", BindingKind::Int);
            let part = parsed_part(&format!("p = (a)-[r:R{range}]->(b)"));
            let result = lower_pattern_part(&mut lowerer, Node::GraphOneRow, &part, optional, None, false).unwrap();
            let Node::GraphApply { kind, correlation, outputs, optional_missing, .. } = result else { panic!("missing apply") };
            assert_eq!(kind, if optional { ApplyKind::Optional } else { ApplyKind::Inner });
            assert_eq!(optional_missing, OptionalMissing::Null);
            assert_eq!(correlation, vec!["a"]);
            assert_eq!(outputs, vec!["b", "p", "r"]);
            for (name, kind) in [("a", BindingKind::Node), ("b", BindingKind::Node), ("p", BindingKind::Path), ("r", expected_rel), ("outside", BindingKind::Int)] {
                assert_eq!(lowerer.binding_kind(name), Some(kind));
            }
            let nullable = &lowerer.scopes.last().unwrap().nullable;
            assert_eq!(nullable, &if optional { BTreeSet::from(["b".into(), "p".into(), "r".into()]) } else { BTreeSet::new() });
        }
    }
}

#[test]
fn pattern_property_dependencies_and_history_are_correlated_once() {
    for pattern in ["(a {v: outside})", "(a)-[r:R {v: outside}]->(b)", "(a)-[r:R]->(b {v: outside})"] {
        for available in [false, true] {
            let mut lowerer = Lowerer::new();
            lowerer.add_visible_kind("outside", BindingKind::Int);
            lowerer.add_visible_kind("unused", BindingKind::String);
            let part = parsed_part(pattern);
            let result = lower_pattern_part(&mut lowerer, Node::GraphOneRow, &part, false, Some("history"), available).unwrap();
            let Node::GraphApply { correlation, outputs, .. } = result else { panic!("missing apply") };
            let mut expected = vec!["outside".to_owned()];
            if available { expected.push("history".into()); }
            assert_eq!(correlation, expected, "{pattern}");
            assert_eq!(outputs.iter().filter(|name| name.as_str() == "history").count(), usize::from(!part.element.chains.is_empty()));
            assert!(!lowerer.is_visible("history"), "internal history must not leak into user scope");
            assert!(!outputs.iter().any(|name| name == "outside" || name == "unused"));
        }
    }
}

#[test]
fn reused_relationships_remain_correlated_without_redeclaring_their_binding() {
    for (range, kind) in [("", BindingKind::Relationship), ("*1..2", BindingKind::RecursiveRelationship), ("*1..2", BindingKind::ListRelationship)] {
        for direction in ["->", "-"] {
            let mut lowerer = Lowerer::new();
            lowerer.add_visible_kind("r", kind);
            let part = parsed_part(&format!("(a)-[r:R{range}]{direction}(b)"));
            let result = lower_pattern_part(&mut lowerer, Node::GraphOneRow, &part, false, None, false).unwrap();
            let Node::GraphApply { correlation, outputs, right, .. } = result else { panic!("missing apply") };
            assert_eq!(correlation, vec!["r"]);
            assert_eq!(outputs, vec!["a", "b"]);
            assert_eq!(lowerer.binding_kind("r"), Some(kind));
            // Equality is a filter on the existing value, never a new output alias.
            let Node::GraphFilter { condition: IrExpr::Binary { op: IrBinaryOp::Eq, .. }, .. } = *right else { panic!("reused relationship must be checked for equality") };
        }
    }
}

#[test]
fn failed_pattern_query_can_be_followed_by_a_valid_query_on_the_same_lowerer() {
    use crate::language::cypher::parser::parse_query;
    for query in [
        "MATCH p = (a) MATCH p = (b) RETURN p",
        "MATCH (a)-[r]->(b) MATCH (r) RETURN r",
        "MATCH (a)-[r]->(b) MATCH (x)-[r*1..2]->(y) RETURN y",
        "MATCH p = (p)-[r]->(b) RETURN p",
        "MATCH (a {v: missing}) RETURN a",
        "MATCH (a)-[r {v: missing}]->(b) RETURN b",
        "MATCH (a)-[r*1..2 {v: missing}]->(b) RETURN b",
    ] {
        let mut lowerer = Lowerer::new();
        let original = lowerer.visible_set();
        let original_depth = lowerer.scopes.len();
        assert!(lowerer.lower_query_body(&parse_query(query).unwrap()).is_err(), "{query}");
        assert_eq!(lowerer.visible_set(), original, "{query}");
        assert_eq!(lowerer.scopes.len(), original_depth, "scope leak: {query}");
        assert!(lowerer.traversal_stack.is_empty(), "traversal leak: {query}");
        let result = lowerer.lower_query_body(&parse_query("MATCH (n) RETURN n").unwrap()).unwrap();
        let Node::GraphReturn { fields, .. } = result else { panic!("missing return after {query}") };
        assert_eq!(fields, vec!["n"]);
        assert_eq!(lowerer.scopes.len(), original_depth);
        assert!(lowerer.traversal_stack.is_empty());
    }
}

#[test]
fn relationship_projection_preserves_input_when_no_path_or_expanded_binding_exists() {
    let input=Node::GraphCorrelate{bindings:vec!["outside".into()]};
    for visible in [false,true] {
        let actual=apply_relationship_binding_projection(input.clone(),"rel",None,None,"source",visible,true);
        assert_eq!(actual,input);
        let actual=apply_relationship_binding_projection(input.clone(),"rel",None,None,"source",visible,false);
        assert_eq!(actual,input);
    }
    let actual=apply_relationship_binding_projection(input.clone(),"rel",Some("expanded"),None,"source",true,false);
    let Node::GraphFilter{condition,input:child}=actual else {panic!("identity filter")};
    assert_eq!(*child,input);
    assert_eq!(condition,IrExpr::Binary{op:IrBinaryOp::Eq,lhs:Box::new(IrExpr::Binding("expanded".into())),rhs:Box::new(IrExpr::Binding("rel".into()))});
}

#[test]
fn recursive_predicate_failure_restores_scope_and_absent_predicate_preserves_input() {
    let mut lowerer=Lowerer::new();let scope=BTreeSet::from(["path".into(),"source".into(),"outside".into()]);
    lowerer.replace_scope(scope.iter().cloned());
    let input=Node::GraphCorrelate{bindings:scope.iter().cloned().collect()};
    let mut recursive=RecursiveRelationshipPattern{rel_variable:"edge".into(),node_variable:"vertex".into(),predicate:None,rel_projection_keys:None,node_projection_keys:None};
    assert_eq!(apply_recursive_relationship_predicate(&mut lowerer,input.clone(),"path","source",&recursive,&scope).unwrap(),input);
    recursive.predicate=Some(Expr::Function{name:"coverage_unknown_recursive_function".into(),distinct:false,args:vec![]});
    let error=apply_recursive_relationship_predicate(&mut lowerer,input,"path","source",&recursive,&scope).unwrap_err();
    assert!(error.to_string().contains("coverage_unknown_recursive_function"),"{error}");assert_eq!(lowerer.visible_set(),scope);
}

#[test]
fn recursive_property_projection_retains_path_identity_and_explicit_key_order() {
    let input=Node::GraphCorrelate{bindings:vec!["path".into()]};
    let mut recursive=RecursiveRelationshipPattern{rel_variable:"edge".into(),node_variable:"vertex".into(),predicate:None,rel_projection_keys:None,node_projection_keys:None};
    assert_eq!(apply_recursive_relationship_projection(input.clone(),"path",&recursive),input);
    for keys in [vec![],vec!["weight".to_string(),"display name".to_string()]] {
        recursive.rel_projection_keys=Some(keys.clone());
        let result=apply_recursive_relationship_projection(input.clone(),"path",&recursive);
        let Node::GraphProject{mode,items,error_policy,input:child}=result else {panic!("path projection")};
        assert_eq!(mode,ProjectMode::PreserveVisible);assert_eq!(error_policy,ProjectErrorPolicy::PropagateError);assert_eq!(*child,input);
        assert_eq!(items,vec![ProjectionItem{alias:"path".into(),expr:IrExpr::Call{name:"path_project_edges".into(),args:vec![IrExpr::Binding("path".into()),IrExpr::List(keys.into_iter().map(|key|IrExpr::Lit(Lit::String(key))).collect())]}}]);
    }
}

#[test]
fn scalar_pattern_property_predicates_preserve_caller_visibility() {
    let mut lowerer=Lowerer::new();let scope=BTreeSet::from(["outside".into(),"path".into(),"source".into()]);
    lowerer.replace_scope(scope.iter().cloned());
    let input=Node::GraphCorrelate{bindings:scope.iter().cloned().collect()};
    let predicate=Expr::Literal(Literal::Bool(true));
    let Node::GraphFilter{condition,input:child}=apply_property_filters(&mut lowerer,input.clone(),"person",&predicate,&scope).unwrap() else {panic!("property filter")};
    assert_eq!(condition,IrExpr::Lit(Lit::Bool(true)));assert_eq!(*child,input);assert_eq!(lowerer.visible_set(),scope);
    let result=apply_variable_length_property_filters(&mut lowerer,input,"path","source",&predicate,&scope).unwrap();
    assert!(matches!(result,Node::GraphFilter{..}));assert_eq!(lowerer.visible_set(),scope);
}
