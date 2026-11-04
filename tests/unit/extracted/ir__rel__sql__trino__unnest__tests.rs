
    use super::*;
    fn query(sql: &str) -> Box<ast::Query> {
        let ast::Statement::Query(query) = Parser::parse_sql(&TrinoParser, sql).unwrap().remove(0)
            else { panic!("query expected") };
        query
    }
    #[test]
    fn paired_expansions_share_one_zip_relation_and_keep_output_aliases() {
        let mut query = query("SELECT unnest(a) AS x, unnest(b) AS y FROM input");
        rewrite(&mut query).unwrap();
        let ast::SetExpr::Select(select) = query.body.as_ref() else { panic!() };
        assert_eq!(select.from.len(), 2);
        let (arity, alias) = match &select.from[1].relation {
            ast::TableFactor::UNNEST { array_exprs, alias, .. } => (array_exprs.len(), alias),
            ast::TableFactor::Table { args: Some(args), alias, .. } => (args.args.len(), alias),
            other => panic!("unexpected expansion: {other:?}"),
        };
        assert_eq!(arity, 2);
        assert_eq!(alias.as_ref().unwrap().columns.len(), 2);
        assert!(query.to_string().contains(" AS x"));
        assert!(query.to_string().contains(" AS y"));
        let once = query.to_string();
        rewrite(&mut query).unwrap();
        assert_eq!(query.to_string(), once);
    }
    #[test]
    fn expansion_names_do_not_shadow_existing_identifiers() {
        let mut query = query("SELECT __orchid_trino_expand_0, unnest(a) AS x FROM input");
        rewrite(&mut query).unwrap();
        let ast::SetExpr::Select(select) = query.body.as_ref() else { panic!() };
        let alias = match &select.from[1].relation {
            ast::TableFactor::UNNEST { alias, .. } | ast::TableFactor::Table { alias, .. } => alias,
            other => panic!("unexpected expansion: {other:?}"),
        };
        assert_eq!(alias.as_ref().unwrap().name.value, "__orchid_trino_expand_1");
    }
    #[test]
    fn union_arms_expand_independently_and_wrap_row_elements() {
        let mut query = query("SELECT unnest(ARRAY[ROW(1, 2)]) AS x UNION ALL SELECT unnest(ARRAY[ROW(3, 4)]) AS x");
        rewrite(&mut query).unwrap();
        let sql = query.to_string();
        assert_eq!(sql.matches("FROM UNNEST(transform(").count(), 2);
        assert_eq!(sql.matches(" -> ROW(").count(), 2);
        Parser::parse_sql(&TrinoParser, &sql).unwrap();
    }
    #[test]
    fn scalar_consumers_and_ambiguous_unnest_arguments_are_rejected() {
        for sql in ["SELECT 1 + unnest(a) FROM input", "SELECT unnest(a, b) FROM input"] {
            assert!(rewrite(&mut query(sql)).is_err(), "{sql}");
        }
    }
