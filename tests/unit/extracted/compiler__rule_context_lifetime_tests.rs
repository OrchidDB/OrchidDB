
    use super::*;
    #[test]
    fn public_prepare_graph_has_a_fresh_query_budget() {
        let mut request: CompileRequest = serde_json::from_value(serde_json::json!({
            "version":1,"dialect":"duckdb","language":"cypher","query":"RETURN 1 AS x"
        }))
        .unwrap();
        request.rule_context.budget.query_visits = 16;
        request.rule_context.budget.stage_visits = 16;
        for _ in 0..64 {
            prepare_graph(&request).unwrap();
        }
        assert_eq!(request.rule_context.visits(), 0);
    }
