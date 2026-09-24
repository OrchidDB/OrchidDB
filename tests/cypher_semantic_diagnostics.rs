use new_graph::language::cypher::parser::parse_query;
use new_graph::language::cypher::planner::{CypherPlanError, CypherPlanner, CypherSemanticError};

#[test]
fn exact_semantic_validators_publish_structured_categories() {
    for (query, detail) in [
        ("RETURN missing", "UndefinedVariable"),
        ("MATCH (n)-[n:R]->() RETURN n", "VariableTypeConflict"),
        (
            "MATCH p = (a)-->(b) WITH p MATCH p = ()-->() RETURN p",
            "VariableAlreadyBound",
        ),
        ("RETURN sum(count(*))", "NestedAggregation"),
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
