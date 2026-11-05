
    use super::*;
    use crate::language::cypher::ast::ProjectionItem as AstProjectionItem;

    fn projection_body(items: Vec<AstProjectionItem>) -> ProjectionBody {
        ProjectionBody {
            distinct: false,
            include_existing: false,
            items,
            order_by: Vec::new(),
            skip: None,
            limit: None,
        }
    }

    fn property(target: Expr, key: &str) -> Expr {
        Expr::Property {
            target: Box::new(target),
            key: key.to_string(),
        }
    }

    fn aliased(expr: Expr, alias: &str) -> AstProjectionItem {
        AstProjectionItem {
            expr,
            alias: Some(alias.to_string()),
            explicit_alias: true,
        }
    }

    #[test]
    fn with_where_using_incoming_variable_filters_before_projection() {
        let mut lowerer = Lowerer::new();
        lowerer.add_visible("a");
        let body = projection_body(vec![aliased(
            property(Expr::Variable("a".to_string()), "name"),
            "name",
        )]);
        let predicate = Expr::Binary {
            op: BinaryOp::Eq,
            lhs: Box::new(property(Expr::Variable("a".to_string()), "name")),
            rhs: Box::new(Expr::Literal(Literal::String("B".to_string()))),
        };

        assert!(matches!(
            lower_with_predicate_placement(&lowerer, &body, &predicate).unwrap(),
            WithPredicatePlacement::BeforeProjection(_)
        ));
    }

    #[test]
    fn with_where_using_aggregate_alias_filters_after_projection() {
        let lowerer = Lowerer::new();
        let body = projection_body(vec![aliased(Expr::CountStar, "count")]);
        let predicate = Expr::Binary {
            op: BinaryOp::Gt,
            lhs: Box::new(Expr::Variable("count".to_string())),
            rhs: Box::new(Expr::Literal(Literal::Integer("0".to_string()))),
        };

        assert!(matches!(
            lower_with_predicate_placement(&lowerer, &body, &predicate).unwrap(),
            WithPredicatePlacement::AfterProjection
        ));
    }

    #[test]
    fn literal_float_slice_bound_is_invalid() {
        let err = literal_u64(&Expr::Literal(Literal::Float(1.5))).unwrap_err();
        assert!(format!("{err}").contains(
            "Runtime exception: The number of rows to skip/limit must be a non-negative integer."
        ));
    }

#[test]
fn ast_boundary_implicit_projection_names_preserve_variables_and_allocate_computed_outputs() {
    let variable = AstProjectionItem { expr: Expr::Variable("value".into()), alias: None, explicit_alias: false };
    let constant = AstProjectionItem { expr: Expr::Literal(Literal::Integer("7".into())), alias: None, explicit_alias: false };
    let mut lowerer = Lowerer::new();
    lowerer.add_visible("value");
    let body = projection_body(vec![variable.clone(), constant.clone()]);
    let (plan, fields) = lower_projection_body(&mut lowerer, Node::GraphOneRow, &body, true).unwrap();
    assert_eq!(fields.len(), 2);
    assert_eq!(fields[0], "value");
    assert_ne!(fields[1], "value");
    let Node::GraphProject { items, .. } = plan else { panic!("expected row projection") };
    assert_eq!(items[0].alias, "value");
    assert_eq!(items[0].expr, IrExpr::Binding("value".into()));
    assert_eq!(items[1].alias, fields[1]);
    assert!(validate_with_projection_aliases(&projection_body(vec![variable])).is_ok());
    assert!(validate_with_projection_aliases(&projection_body(vec![constant])).unwrap_err().to_string().contains("must be aliased"));
}

#[test]
fn ast_boundary_implicit_projection_aliases_distinguish_passthrough_shadowing_and_unnamed_values() {
    let body = projection_body(vec![
        AstProjectionItem { expr: Expr::Variable("source".into()), alias: None, explicit_alias: false },
        aliased(Expr::Variable("source".into()), "target"),
        AstProjectionItem { expr: Expr::Literal(Literal::Integer("1".into())), alias: None, explicit_alias: false },
    ]);
    assert_eq!(projection_aliases(&body), BTreeMap::from([
        ("source".into(), Expr::Variable("source".into())),
        ("target".into(), Expr::Variable("source".into())),
    ]));
    assert_eq!(shadowed_projection_fields(&body, &["source".into(), "target".into()]), BTreeSet::from(["target".into()]));
    let candidates = BTreeSet::from(["outer".into()]);
    let mut references = BTreeSet::from(["source".into(), "target".into(), "outer".into()]);
    scope::remove_local_projection_bindings(&body, &candidates, &mut references);
    assert_eq!(references, candidates);
}
