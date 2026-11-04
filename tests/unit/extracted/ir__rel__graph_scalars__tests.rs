
    use super::*;

    #[tokio::test]
    async fn flat_terminal_columns_decode_back_to_graph_references() {
        let graph = PropertyGraph::new();
        let a = graph.insert_node("A", BTreeMap::new());
        let b = graph.insert_node("B", BTreeMap::new());
        let edge = graph.insert_edge("T", &a, &b, BTreeMap::new()).unwrap();
        for (query, expected_fields, expected) in [
            ("MATCH (a:A)-[r:T]->(b:B) RETURN b,r,a", vec!["b", "r", "a"], vec![b.clone(), edge, a]),
            ("MATCH (b:B) OPTIONAL MATCH (b)-[r:T]->(n) RETURN n,r,b", vec!["n", "r", "b"], vec![Value::Null, Value::Null, b]),
        ] {
            let ast = crate::language::cypher::parser::parse_query(query).unwrap();
            let plan = crate::language::cypher::planner::CypherPlanner::new().plan(&ast).unwrap();
            let lowered = RelBackend::new().with_language_functions(true).lower(&plan, &graph).unwrap();
            let returned = sql::execute_lowered_sql(&mut sql::DuckDbExecutor::default(), &lowered).await.unwrap();
            let (fields, rows) = crate::ir::exec::batch_to_bindings(&returned).unwrap();
            assert_eq!(fields, expected_fields);
            assert_eq!(rows, vec![expected]);
        }
    }
