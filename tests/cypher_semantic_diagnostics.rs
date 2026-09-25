use new_graph::language::cypher::parser::parse_query;
use new_graph::language::cypher::planner::{CypherPlanError, CypherPlanner, CypherSemanticError};

#[test]
fn exact_semantic_validators_publish_structured_categories() {
    for (query, detail) in [
        ("RETURN missing", "UndefinedVariable"),
        ("RETURN 1 AS a, 2 AS a", "ColumnNameConflict"),
        ("WITH 1 AS a, 2 AS a RETURN a", "ColumnNameConflict"),
        ("MATCH (a) WITH a, count(*) RETURN a", "NoExpressionAlias"),
        ("RETURN 1 AS a UNION RETURN 2 AS b", "DifferentColumnsInUnion"),
        ("RETURN 1 AS a UNION ALL RETURN 2 AS b", "DifferentColumnsInUnion"),
        ("RETURN true AND 12", "InvalidArgumentType"),
        ("RETURN none(x IN ['Clara'] WHERE x % 2 = 0)", "InvalidArgumentType"),
        ("RETURN any(x IN [true, false] WHERE x % 2 = 0)", "InvalidArgumentType"),
        ("MATCH (n)-[n:R]->() RETURN n", "VariableTypeConflict"),
        (
            "MATCH p = (a)-->(b) WITH p MATCH p = ()-->() RETURN p",
            "VariableAlreadyBound",
        ),
        ("RETURN sum(count(*))", "NestedAggregation"),
        ("MATCH (n) RETURN [x IN [1,2,3] | count(*)]", "InvalidAggregation"),
        ("MATCH (n) RETURN any(x IN [1,2] WHERE count(*) > x)", "InvalidAggregation"),
        (
            "MATCH (n) WHERE count(n) > 0 RETURN n",
            "InvalidAggregation",
        ),
    ] {
        let query_ast = parse_query(query).unwrap();
        let error = CypherPlanner::new().plan(&query_ast).unwrap_err();
        assert_eq!(
            error.classification(),
            Some(("SyntaxError", detail)),
            "{query}: {error}"
        );
    }
}

#[test]
fn structured_category_keeps_existing_diagnostic_text() {
    let original = CypherPlanError::Unsupported("original diagnostic".into());
    let display = original.to_string();
    let classified = original.classified(CypherSemanticError::NestedAggregation);
    assert_eq!(classified.to_string(), display);
    assert_eq!(
        CypherPlanError::Unsupported("unimplemented feature".into()).classification(),
        None
    );
    assert_eq!(
        CypherPlanError::Invalid("generic plan error".into()).classification(),
        None
    );
}

#[test]
fn aggregate_collection_is_allowed_outside_iteration_body() {
    let query = parse_query("MATCH (n) RETURN [x IN collect(n) | x]").unwrap();
    CypherPlanner::new().plan(&query).unwrap();
}
