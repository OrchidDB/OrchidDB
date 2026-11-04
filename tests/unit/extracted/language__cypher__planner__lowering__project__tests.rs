
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
