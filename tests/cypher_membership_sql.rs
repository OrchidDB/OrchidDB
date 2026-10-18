#![cfg(feature = "duckdb")]
use orchiddb::ir::{catalog::PropertyGraph, rel::{RelBackend, sql::{DuckDbExecutor, execute_lowered_sql}}};
use orchiddb::language::cypher::{parser::parse_query, planner::CypherPlanner};

#[tokio::test]
async fn computed_boolean_membership_preserves_three_valued_precedence() {
    for op in ["=", "<>", "<", "<=", ">", ">=", "AND", "OR", "XOR"] {
        let query = format!("UNWIND [true,false,null] AS a UNWIND [true,false,null] AS b UNWIND [[],[true],[false],[null],[true,false],[true,false,null]] AS c WITH collect((a {op} b IN c) = (a {op} (b IN c))) AS eq, collect((a {op} b IN c) <> ((a {op} b) IN c)) AS neq RETURN all(x IN eq WHERE x) AND any(x IN neq WHERE x)");
        let plan = CypherPlanner::new().plan(&parse_query(&query).unwrap()).unwrap();
        let lowered = RelBackend::new().with_language_functions(true).lower(&plan, &PropertyGraph::new()).unwrap_or_else(|e| panic!("{op}: {e}"));
        let result = execute_lowered_sql(&mut DuckDbExecutor::default(), &lowered).await.unwrap_or_else(|e| panic!("{op}: {e}"));
        assert_eq!(arrow::util::display::array_value_to_string(result.batch.column(0), 0).unwrap(), "true", "{op}");
    }
}

#[test]
fn volatile_and_throwing_membership_inputs_still_require_runtime() {
    for query in ["RETURN rand() IN [0,1]", "RETURN (1/0) IN []", "RETURN size(1) IN []", "RETURN rand() IN null", "UNWIND [0] AS n RETURN (1/n) IN null"] {
        let plan = CypherPlanner::new().plan(&parse_query(query).unwrap()).unwrap();
        assert!(RelBackend::new().with_language_functions(true).lower(&plan, &PropertyGraph::new()).is_err(), "{query}");
    }
}

#[test]
fn nested_null_membership_keeps_three_valued_runtime_equality() {
    let query = "UNWIND [[[null]]] AS lists UNWIND [[null]] AS x RETURN x IN lists";
    let plan = CypherPlanner::new().plan(&parse_query(query).unwrap()).unwrap();
    assert!(RelBackend::new().with_language_functions(true).lower(&plan, &PropertyGraph::new()).is_err());
}

#[tokio::test]
async fn extreme_subscript_and_folded_graph_key_are_lossless() {
    use std::collections::BTreeMap;
    use orchiddb::ir::Value;
    let graph = PropertyGraph::new();
    graph.insert_node("N", BTreeMap::from([("name".into(), Value::String("Apa".into()))]));
    for (query, expected) in [
        ("UNWIND [[1]] AS xs RETURN xs[9223372036854775807] IN [1]", ""),
        ("UNWIND [[1]] AS xs RETURN xs[-9223372036854775808]", ""),
        ("MATCH (n:N) RETURN n['nam'+'e']", "Apa"),
    ] {
        let plan = CypherPlanner::new().plan(&parse_query(query).unwrap()).unwrap();
        let lowered = RelBackend::new().with_language_functions(true).lower(&plan, &graph).unwrap();
        let result = execute_lowered_sql(&mut DuckDbExecutor::default(), &lowered).await.unwrap();
        assert_eq!(arrow::util::display::array_value_to_string(result.batch.column(0), 0).unwrap(), expected, "{query}");
    }
}
