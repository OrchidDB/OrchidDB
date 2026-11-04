
    use super::*;
    use datafusion::sql::sqlparser::{dialect::DuckDbDialect, parser::Parser};
    #[test]
    fn exact_identity_aliases_are_removed_in_nested_and_union_queries() {
        let mut statement = Parser::parse_sql(&DuckDbDialect {},
            "WITH q AS (SELECT \"X\" AS \"X\" FROM t) SELECT \"X\" AS \"X\" FROM q UNION ALL SELECT \"X\" AS \"X\" FROM q").unwrap().remove(0);
        strip_identity_aliases(&mut statement);
        assert_eq!(statement.to_string(),
            "WITH q AS (SELECT \"X\" FROM t) SELECT \"X\" FROM q UNION ALL SELECT \"X\" FROM q");
    }
    #[test]
    fn distinct_spelling_qualification_and_computation_keep_aliases() {
        let input = "SELECT X AS \"X\", \"x\" AS \"X\", t.x AS x, (x + 1) AS x FROM t";
        let mut statement = Parser::parse_sql(&DuckDbDialect {}, input).unwrap().remove(0);
        let original = statement.clone();
        strip_identity_aliases(&mut statement);
        assert_eq!(statement, original);
    }
