#![cfg(feature = "duckdb")]
use orchiddb::engine::{GraphEngine, QueryResult};
fn rows(r: &QueryResult) -> Vec<String> {
    let b = &r.returned.batch;
    (0..b.num_rows())
        .map(|i| {
            b.columns()
                .iter()
                .map(|a| arrow::util::display::array_value_to_string(a, i).unwrap())
                .collect::<Vec<_>>()
                .join("|")
        })
        .collect()
}

#[tokio::test]
async fn logical_labels_lower_with_aliases_nulls_and_relationship_types() {
    let mut e = GraphEngine::in_memory().unwrap();
    e.cypher("CREATE (a:A:B {k:1}),(b:A {k:2}),({k:3}),(a)-[:T]->(b)")
        .await
        .unwrap();
    for (query, expected) in [
        (
            "MATCH (n) RETURN n.k,n:A,n:B,labels(n) ORDER BY n.k",
            vec!["1|true|true|[A,B]", "2|true|false|[A]", "3|false|false|[]"],
        ),
        ("MATCH (n) WITH n AS m WHERE m:A:B RETURN m.k", vec!["1"]),
        ("MATCH (n) WHERE NOT n:A RETURN n.k", vec!["3"]),
        ("MATCH (n) WHERE size(labels(n)) = 0 RETURN n.k", vec!["3"]),
        (
            "MATCH (n) UNWIND labels(n) AS l RETURN l,count(*) ORDER BY l",
            vec!["A|2", "B|1"],
        ),
        ("MATCH ()-[r]->() RETURN r:T,r:Missing", vec!["true|false"]),
        (
            "MATCH (n:B) OPTIONAL MATCH (n)-[:Missing]->(m) RETURN m:A,labels(m)",
            vec!["|"],
        ),
        (
            "MATCH (n:B) OPTIONAL MATCH (n)-[:T]->(m) WHERE m:B RETURN n.k,m:A",
            vec!["1|"],
        ),
        (
            "MATCH (n:B)-[:T]->(m) WITH m AS n RETURN n:A,n:B,labels(n)",
            vec!["true|false|[A]"],
        ),
        (
            "MATCH (n) WITH DISTINCT n RETURN n.k,n:B ORDER BY n.k",
            vec!["1|true", "2|false", "3|false"],
        ),
    ] {
        let r = e.cypher(query).await.unwrap();
        assert_eq!(rows(&r), expected, "{query}\n{:?}", r.stats.sql_queries);
        assert_eq!(
            r.stats.cost.native_relational_kernel_calls, 0,
            "{query}\n{}",
            r.stats.physical_plan
        );
    }
    e.cypher("MATCH (n:B) SET n:C REMOVE n:A").await.unwrap();
    let r = e
        .cypher("MATCH (n) WHERE n:B RETURN n:A,n:B,n:C,labels(n)")
        .await
        .unwrap();
    assert_eq!(rows(&r), ["false|true|true|[B,C]"]);
    assert_eq!(r.stats.cost.native_relational_kernel_calls, 0);
    assert!(e.cypher("RETURN labels(42)").await.is_err());
}

#[tokio::test]
async fn label_membership_is_plain_sql_with_native_values_disabled() {
    use orchiddb::ir::{
        catalog::PropertyGraph,
        rel::{
            RelBackend,
            sql::{DuckDbExecutor, execute_lowered_sql},
        },
    };
    use orchiddb::language::cypher::{parser::parse_query, planner::CypherPlanner};
    let graph = PropertyGraph::new();
    let node = graph.insert_node("Storage", std::collections::BTreeMap::new());
    graph
        .set_node_labels(&node, ["A".into(), "B".into()])
        .unwrap();
    let second = graph.insert_node("Storage", std::collections::BTreeMap::new());
    graph.set_node_labels(&second, ["A".into()]).unwrap();
    let plan = CypherPlanner::new()
        .plan(
            &parse_query("MATCH (n) RETURN n:A,n:B,n:Storage,labels(n) ORDER BY n:B DESC").unwrap(),
        )
        .unwrap();
    let lowered = RelBackend::new()
        .with_language_functions(true)
        .lower(&plan, &graph)
        .unwrap();
    let simple = CypherPlanner::new()
        .plan(&parse_query("MATCH (n) RETURN count(*)").unwrap())
        .unwrap();
    let simple = RelBackend::new().lower(&simple, &graph).unwrap();
    use datafusion::common::tree_node::{TreeNode, TreeNodeRecursion};
    simple
        .plan
        .apply(|node| {
            if let datafusion::logical_expr::LogicalPlan::TableScan(scan) = node {
                assert!(
                    !scan
                        .source
                        .schema()
                        .fields()
                        .iter()
                        .any(|field| field.name().ends_with("__cypher_labels"))
                );
            }
            Ok(TreeNodeRecursion::Continue)
        })
        .unwrap();
    let result = execute_lowered_sql(&mut DuckDbExecutor::default(), &lowered)
        .await
        .unwrap();
    let actual = result
        .batch
        .columns()
        .iter()
        .map(|a| arrow::util::display::array_value_to_string(a, 0).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(actual, ["true", "true", "false", "[A, B]"]);
    let second = result
        .batch
        .columns()
        .iter()
        .map(|a| arrow::util::display::array_value_to_string(a, 1).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(second, ["true", "false", "false", "[A]"]);
}
