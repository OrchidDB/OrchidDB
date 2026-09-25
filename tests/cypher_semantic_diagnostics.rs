use new_graph::language::cypher::parser::parse_query;
use new_graph::language::cypher::planner::{CypherPlanError, CypherPlanner, CypherSemanticError};

#[test]
fn exact_semantic_validators_publish_structured_categories() {
    for (query, detail) in [
        ("RETURN missing", "UndefinedVariable"),
        ("MATCH (n) WHERE n RETURN n", "InvalidArgumentType"),
        ("MATCH (n) RETURN (n)-->()", "UnexpectedSyntax"),
        ("MATCH (n) RETURN [(n)-->(m) WHERE m | m]", "InvalidArgumentType"),
        ("MATCH (n) RETURN foo(n)", "UnknownFunction"),
        ("MATCH (n) RETURN n.x + count(*)", "AmbiguousAggregationExpression"),
        ("MATCH (n) WITH n.x + n.y, count(*) AS c ORDER BY n.x + n.y + count(*) RETURN c", "AmbiguousAggregationExpression"),
        ("MATCH (n) WITH n.x AS x ORDER BY n, count(*) RETURN x", "InvalidAggregation"),
        ("RETURN 1.34E999", "FloatingPointOverflow"),
        ("MATCH () RETURN *", "NoVariablesInScope"),
        ("MATCH (n) RETURN length(n)", "InvalidArgumentType"),
        ("RETURN properties([true,false])", "InvalidArgumentType"),
        ("MATCH (n) RETURN size((n)--())", "UnexpectedSyntax"),
        ("RETURN 9223372036854775808", "IntegerOverflow"),
        ("RETURN -9223372036854775809", "IntegerOverflow"),
        ("RETURN 0x8000000000000000", "IntegerOverflow"),
        ("RETURN -0o1000000000000000000001", "IntegerOverflow"),
        ("CREATE ()-->()", "NoSingleRelationshipType"),
        ("MERGE ()-[:A|:B]->()", "NoSingleRelationshipType"),
        ("RETURN 1 AS a, 2 AS a", "ColumnNameConflict"),
        ("WITH 1 AS a, 2 AS a RETURN a", "ColumnNameConflict"),
        ("MATCH (a) WITH a, count(*) RETURN a", "NoExpressionAlias"),
        ("RETURN 1 AS a UNION RETURN 2 AS b", "DifferentColumnsInUnion"),
        ("RETURN 1 AS a UNION ALL RETURN 2 AS b", "DifferentColumnsInUnion"),
        ("RETURN true AND 12", "InvalidArgumentType"),
        ("RETURN 1 IN true", "InvalidArgumentType"),
        ("RETURN 1 IN {}", "InvalidArgumentType"),
        ("MATCH p = ()-->() RETURN size(p)", "InvalidArgumentType"),
        ("MATCH (n) RETURN n SKIP n.count", "NonConstantExpression"),
        ("RETURN 1 LIMIT -1", "NegativeIntegerArgument"),
        ("RETURN 1 LIMIT 1.5", "InvalidArgumentType"),
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

#[test]
fn pattern_predicates_are_valid_in_predicate_positions() {
    for query in [
        "MATCH (n) WHERE (n)-->() RETURN n",
        "MATCH (n) RETURN [(n)-->(m) WHERE (m)-->() | m]",
        "MATCH (n) RETURN NOT (n)-->() AS absent",
    ] {
        let parsed = parse_query(query).unwrap();
        CypherPlanner::new().plan(&parsed).unwrap_or_else(|error| panic!("{query}: {error}"));
    }
}

#[test]
fn parser_reports_lexical_and_syntax_categories() {
    for query in ["RETURN [, ]", "RETURN [1,,2]", "RETURN [1,]"] {
        assert_eq!(parse_query(query).unwrap_err().classification(), Some(("SyntaxError", "UnexpectedSyntax")), "{query}");
    }
    for query in ["RETURN 123abc", "RETURN 0x", "RETURN 0x1A2b3j4D5E6f7"] {
        let error=parse_query(query).unwrap_err();
        assert_eq!(error.classification(),Some(("SyntaxError","InvalidNumberLiteral")),"{query}: {error}");
    }
    assert_eq!(parse_query("RETURN (").unwrap_err().classification(),Some(("SyntaxError","UnexpectedSyntax")));
    parse_query("RETURN '123abc', 12 /*abc*/ AS n").unwrap();
}

#[test]
fn known_non_property_values_have_type_diagnostics() {
    for value in ["123","42.5","true","'string'","[123,true]"] {
        let query=parse_query(&format!("WITH {value} AS x RETURN x.num")).unwrap();
        let error=CypherPlanner::new().plan(&query).unwrap_err();
        assert_eq!(error.classification(),Some(("TypeError","InvalidArgumentType")),"{error}");
    }
}
