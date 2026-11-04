use super::*;
use crate::language::cypher::parser::parse_query;

fn expression(text: &str) -> Expr {
    let q = parse_query(&format!("RETURN {text} AS result")).unwrap();
    let Clause::Return(c) = &q.clauses[0] else { panic!() };
    c.projection.items[0].expr.clone()
}

#[test]
fn nested_static_boolean_errors_propagate_through_expression_containers() {
    for text in [
        "NOT (NOT 1)", "(NOT 1) + 2", "2 + (NOT 1)", "true XOR (NOT 1)",
        "coalesce(null, NOT 1)", "[true, NOT 1]", "{a:NOT 1}",
        "CASE (NOT 1) WHEN true THEN 1 ELSE 2 END",
        "CASE WHEN (NOT 1) THEN 1 ELSE 2 END",
        "CASE WHEN true THEN (NOT 1) ELSE 2 END",
        "CASE WHEN true THEN 1 ELSE (NOT 1) END",
        "[x IN [NOT 1] | x]", "[x IN [1] WHERE NOT 1 | x]", "[x IN [1] | NOT 1]",
        "list_transform([1], x -> NOT 1)", "list_filter([1], x -> NOT 1)",
        "list_reduce([1], (acc,x) -> NOT 1)", "all(x IN [1] WHERE NOT 1)",
        "[(a)-->(b) WHERE NOT 1 | b]", "[(a)-->(b) | NOT 1]",
    ] {
        let error = validate_static_expression_types(&expression(text)).expect_err(text).to_string();
        assert!(error.contains("INT64") && error.contains("BOOL"), "{text}: {error}");
    }
}

#[test]
fn static_type_diagnostics_preserve_nested_shapes_and_dynamic_fields() {
    for (text, expected) in [
        ("[]", "INT64[]"), ("[[]]", "INT64[][]"), ("[null]", "INT64[]"),
        ("[1,2.0]", "INT64[]"), ("[2.0,1]", "DOUBLE[]"), ("[1,'a']", "ANY[]"),
        ("{a:1,b:unknown,c:[true]}", "STRUCT(a INT64, b ANY, c BOOL[])"),
    ] {
        assert_eq!(static_expr_type_name(&expression(text)).unwrap().as_deref(), Some(expected), "{text}");
    }
    assert_eq!(display_order_expr(&expression("ID(n)")), "n._ID");
    assert_eq!(display_order_expr(&expression("id(n,m)")), "id(n,m)");
    assert_eq!(display_semantic_expr(&expression("n.address.city")), "n.address.city");
    assert_eq!(display_literal_expr(&expression("n")), "<expression>");
    assert_eq!(display_semantic_expr(&expression("[x IN xs | x]")), "<expression>");
    for text in ["true", "1.0", "'a'", "{a:1}"] {
        assert!(validate_list_source(&expression(text), &SemanticScope::default()).unwrap_err().to_string().contains("LIST was expected"));
    }
}

#[test]
fn temporal_function_types_handle_cast_domains_without_guessing_dynamic_types() {
    let scope = SemanticScope::default();
    for (text, expected) in [
        ("timestamp('2020-01-01')", BindingKind::Timestamp),
        ("cast(v,'TIMESTAMP')", BindingKind::Timestamp),
        ("cast(v,'timestamp_ns')", BindingKind::Timestamp),
        ("cast(v,'TIMESTAMP_SEC')", BindingKind::Timestamp),
        ("cast(v,'TIMESTAMP_TZ')", BindingKind::Timestamp),
        ("cast(v,'TIMESTAMP_MS')", BindingKind::TimestampMs),
        ("cast(v,'DATE')", BindingKind::Date),
        ("cast(v,'INTERVAL')", BindingKind::Interval),
        ("cast(v,'UNKNOWN')", BindingKind::Value),
        ("cast(v,target)", BindingKind::Value),
    ] {
        assert_eq!(projected_expr_kind(&expression(text), &scope), expected, "{text}");
    }
}

#[test]
fn clause_validation_propagates_nested_errors_before_execution() {
    for text in [
        "CALL custom.rows(NOT 1) YIELD value RETURN value",
        "CALL custom.rows() YIELD value WHERE NOT 1 RETURN value",
        "MERGE (a {v:NOT 1}) RETURN a",
        "MERGE (a) ON CREATE SET a.x = NOT 1 RETURN a",
        "MERGE (a) ON MATCH SET a += {x:NOT 1} RETURN a",
        "CREATE (a {v:NOT 1}) RETURN a",
        "CREATE (a)-[r:R {v:NOT 1}]->(b) RETURN a",
        "CREATE (a)-[r:R]->(b {v:NOT 1}) RETURN a",
        "SET a.x = NOT 1 RETURN a",
        "SET a = {v:NOT 1} RETURN a",
        "SET a += {v:NOT 1} RETURN a",
        "DELETE (NOT 1)",
        "WITH (NOT 1) AS x RETURN x",
        "WITH 1 AS x WHERE NOT 1 RETURN x",
        "RETURN 1 AS x ORDER BY NOT 1",
        "RETURN 1 AS x SKIP (NOT 1)",
        "RETURN 1 AS x LIMIT (NOT 1)",
    ] {
        let query = parse_query(text).unwrap_or_else(|e| panic!("{text}: {e}"));
        let result: CypherPlanResult<Vec<_>> = query.clauses.iter().map(validate_clause_static_expression_types).collect();
        let error = result.expect_err(text).to_string();
        assert!(error.contains("INT64") && error.contains("BOOL"), "{text}: {error}");
    }
}
