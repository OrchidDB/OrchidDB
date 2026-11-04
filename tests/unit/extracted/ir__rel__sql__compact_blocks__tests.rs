
    use super::*;
    fn compact(sql: &str) -> String {
        let mut statements=datafusion::sql::sqlparser::parser::Parser::parse_sql(
            &datafusion::sql::sqlparser::dialect::DuckDbDialect {},sql).unwrap();
        rewrite(&mut statements[0]); statements[0].to_string()
    }
    #[test]
    fn rename_above_sorted_limit_keeps_order_and_limit_in_final_block() {
        assert_eq!(compact("SELECT x AS result FROM (SELECT * FROM t ORDER BY x LIMIT 2)"),
            "SELECT x AS result FROM t ORDER BY x LIMIT 2");
        for sql in [
            "SELECT x AS y FROM (SELECT * FROM t ORDER BY y LIMIT 2)",
            "SELECT x FROM (SELECT * FROM t ORDER BY 2 LIMIT 2)",
            "SELECT DISTINCT x FROM (SELECT * FROM t LIMIT 2)",
            "SELECT x FROM (SELECT * FROM t LIMIT 2) WHERE x > 0",
            "SELECT x FROM (SELECT DISTINCT * FROM t LIMIT 2)",
        ] { assert!(compact(sql).contains("FROM (SELECT"),"{sql}"); }
    }
