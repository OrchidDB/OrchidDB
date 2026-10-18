#![cfg(feature = "duckdb")]

use orchiddb::ir::{catalog::PropertyGraph, rel::{RelBackend, sql::{DuckDbExecutor, execute_lowered_sql}}};
use orchiddb::language::cypher::{parser::parse_query, planner::CypherPlanner};

async fn ordinary_sql(query: &str) -> (Vec<Vec<String>>, String) {
    let plan = CypherPlanner::new().plan(&parse_query(query).unwrap()).unwrap();
    let lowered = RelBackend::new().with_language_functions(true)
        .lower(&plan, &PropertyGraph::new()).unwrap_or_else(|e| panic!("{query}: {e}"));
    let result = execute_lowered_sql(&mut DuckDbExecutor::default(), &lowered)
        .await.unwrap_or_else(|e| panic!("{query}: {e}"));
    let rows = (0..result.batch.num_rows()).map(|row| result.batch.columns().iter()
        .map(|a| arrow::util::display::array_value_to_string(a,row).unwrap()).collect()).collect();
    (rows, format!("{:?}", lowered.plan))
}

#[tokio::test]
async fn nested_lists_preserve_null_empty_and_integer_payloads() {
    let (rows, _) = ordinary_sql("WITH [[],[null],[9223372036854775807],null] AS v RETURN size(v),size(v[0]),size(v[1]),v[2][0],v[3] IS NULL").await;
    assert_eq!(rows, vec![vec!["4".to_string(),"0".to_string(),"1".to_string(),"9223372036854775807".to_string(),"true".to_string()]]);
    let (rows, _) = ordinary_sql("UNWIND [[],[null],[1,2],null] AS x RETURN size(x) AS n").await;
    assert_eq!(rows, vec![vec!["0".to_string()],vec!["1".to_string()],vec!["2".to_string()],vec!["".to_string()]]);
}

#[tokio::test]
async fn literal_quantifiers_skip_heterogeneous_payload_materialization() {
    for (quantifier, predicate, expected) in [
        ("all","true","true"),("all","false","false"),
        ("any","true","true"),("any","false","false"),
        ("none","true","false"),("none","false","true"),
        ("single","true","false"),("single","false","false"),
        ("all","null",""),("single","null",""),
    ] {
        let query = format!("RETURN {quantifier}(x IN [1,'a',[true],null] WHERE {predicate}) AS result");
        let (rows, plan) = ordinary_sql(&query).await;
        assert_eq!(rows, vec![vec![expected.to_string()]], "{query}");
        assert!(!plan.contains("Unnest"), "{plan}");
        assert!(!plan.contains("Window"), "{plan}");
    }
}

#[tokio::test]
async fn literal_map_quantifiers_reuse_null_truth_tables() {
    for (quantifier, expected) in [("all","false"),("any","true"),("none","false"),("single","")] {
        let query = format!("RETURN {quantifier}(x IN [{{a:true}},{{a:null}},{{a:false}}] WHERE x.a)");
        let (rows, plan) = ordinary_sql(&query).await;
        assert_eq!(rows, vec![vec![expected.to_string()]], "{query}");
        assert!(!plan.contains("Unnest"), "{plan}");
    }
}

#[tokio::test]
async fn deeply_nested_literal_uses_one_outer_schema_cast() {
    // Conformance Literals7:154 reaches 39 list levels. Redundant casts at
    // every level used to exhaust the SQL parser's speculative recursion.
    let literal = format!("{}{}", "[".repeat(39), "]".repeat(39));
    let (rows, _) = ordinary_sql(&format!("RETURN {literal} AS literal")).await;
    assert_eq!(rows, vec![vec![literal]]);
}

#[tokio::test]
async fn negative_float_quantifiers_emit_scalar_constants() {
    for (quantifier, expected) in [("all", "true"), ("any", "true"), ("none", "false"), ("single", "true")] {
        for predicate in ["true", "x < -1.0"] {
            let query = format!("RETURN {quantifier}(x IN [-2.1] WHERE {predicate}) AS result");
            let (rows, plan) = ordinary_sql(&query).await;
            assert_eq!(rows, vec![vec![expected.to_string()]], "{query}");
            assert!(!plan.contains("Unnest") && !plan.contains("Window"), "{query}: {plan}");
            let plan = CypherPlanner::new().plan(&parse_query(&query).unwrap()).unwrap();
            let lowered = RelBackend::new().with_language_functions(true)
                .lower(&plan, &PropertyGraph::new()).unwrap();
            let sql = orchiddb::ir::rel::sql::unparse(&lowered, orchiddb::ir::rel::sql::SqlDialect::DuckDb).unwrap();
            assert_eq!(sql, format!("SELECT {expected} AS \"result\""), "{query}");
        }
    }
}
