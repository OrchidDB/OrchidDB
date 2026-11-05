use super::*;
use crate::language::cypher::parser::parse_query;

#[test]
fn arithmetic_type_errors_identify_each_operator_and_operand_domain() {
    for (op, symbol) in [(BinaryOp::Add,"+"), (BinaryOp::Sub,"-"),
        (BinaryOp::Mul,"*"), (BinaryOp::Div,"/")] {
        for (left, right) in [("true", "1"), ("1", "true")] {
            let error = validate_binary_expr_kind(op, &expression(left), &expression(right),
                &SemanticScope::default()).unwrap_err();
            assert_eq!(error.classification(), Some(("SyntaxError", "InvalidArgumentType")));
            assert!(error.to_string().contains(&format!("function {symbol}(")), "{error}");
            assert!(error.to_string().contains("BOOL") && error.to_string().contains("INT64"));
        }
    }
}

#[test]
fn ordered_aggregates_reject_graph_elements_but_accept_scalar_properties() {
    for kind in [BindingKind::Node, BindingKind::Relationship,
        BindingKind::RecursiveRelationship, BindingKind::Path] {
        let mut scope = SemanticScope::default();
        scope.insert("entity", kind);
        for name in ["min", "MAX"] {
            let error = validate_function_expr_kind(name, &[expression("entity")], &scope).unwrap_err();
            assert!(error.to_string().contains(&format!("Function {}", name.to_uppercase())), "{error}");
            validate_function_expr_kind(name, &[expression("entity.weight")], &scope).unwrap();
        }
    }
}

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
        assert_eq!(static_expr_type_name(&expression(text)).as_deref(), Some(expected), "{text}");
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

#[test]
fn temporal_arithmetic_errors_name_the_actual_operand_types() {
    for (kind,name) in [(BindingKind::Date,"DATE"),(BindingKind::Timestamp,"TIMESTAMP"),
        (BindingKind::TimestampMs,"TIMESTAMP_MS"),(BindingKind::Interval,"INTERVAL")] {
        let mut scope=SemanticScope::default();scope.insert("value",kind);
        let error=validate_binary_expr_kind(BinaryOp::Mul,&expression("value"),&expression("true"),&scope).unwrap_err();
        assert_eq!(error.classification(),Some(("SyntaxError","InvalidArgumentType")));
        assert!(error.to_string().contains(&format!("{name},BOOL")),"{error}");
    }
}

#[test]
fn nested_binding_kind_errors_keep_the_original_property_diagnostic() {
    let mut scope=SemanticScope::default();scope.insert("n",BindingKind::Int);
    let baseline=validate_expr_kinds(&expression("n.value"),&scope).unwrap_err();
    assert_eq!(baseline.classification(),Some(("TypeError","InvalidArgumentType")));
    for text in [
        "NOT n.value","n.value IS NULL","n.value IS NOT NULL",
        "n.value + 1","1 + n.value","n.value STARTS WITH 'x'","'x' STARTS WITH n.value",
        "coalesce(null,n.value)","[1,n.value]","{a:1,b:n.value}",
        "CASE n.value WHEN 1 THEN 2 ELSE 3 END",
        "CASE WHEN n.value THEN 1 ELSE 2 END",
        "CASE WHEN true THEN n.value ELSE 2 END",
        "CASE WHEN true THEN 1 ELSE n.value END",
        "[x IN n.value | x]","[x IN [1] WHERE n.value | x]","[x IN [1] | n.value]",
        "list_transform(n.value,x -> x)","list_transform([1],x -> n.value)",
        "list_filter(n.value,x -> true)","list_filter([1],x -> n.value)",
        "list_reduce(n.value,(acc,x) -> acc)","list_reduce([1],(acc,x) -> n.value)",
        "all(x IN n.value WHERE true)","all(x IN [1] WHERE n.value)",
        "[(a)-->(b) WHERE n.value | b]","[(a)-->(b) | n.value]",
    ] {
        let error=validate_expr_kinds(&expression(text),&scope).expect_err(text);
        assert_eq!(error.classification(),baseline.classification(),"{text}: {error}");
        assert_eq!(error.to_string(),baseline.to_string(),"{text}");
    }
    let invalid_collection=validate_expr_kinds(&expression("all(x IN 1 WHERE true)"),&scope).unwrap_err();
    assert!(invalid_collection.to_string().contains("LIST was expected"),"{invalid_collection}");
}

#[test]
fn infallible_literal_inference_preserves_unknown_and_nested_element_types() {
    for (text, expected) in [
        ("[null,unknown]", "INT64[]"), ("[[],[1]]", "INT64[][]"),
        ("[[1],[]]", "INT64[][]"), ("[[null],[true]]", "BOOL[][]"),
        ("[1,[2]]", "ANY[]"), ("[{a:1},{a:true}]", "STRUCT[]"),
        ("[unknown,true]", "BOOL[]"), ("{x:[null,unknown],y:{z:unknown}}", "STRUCT(x INT64[], y STRUCT(z ANY))"),
    ] {
        assert_eq!(static_expr_type_name(&expression(text)).as_deref(),Some(expected),"{text}");
    }
    assert_eq!(infer_literal_list_type(&[]),None);
    assert_eq!(infer_literal_list_type(&[expression("null"),expression("unknown")]),None);
    assert_eq!(static_expr_type_name(&expression("unknown")),None);
}
