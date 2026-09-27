#![cfg(feature = "duckdb")]
use orchiddb::{
    engine::GraphEngine,
    ir::{PropertyGraph, Value},
};

fn modern() -> GraphEngine {
    let graph = PropertyGraph::new();
    let mut nodes = Vec::new();
    for (id, name, label) in [
        (1, "marko", "person"),
        (2, "vadas", "person"),
        (3, "lop", "software"),
        (4, "josh", "person"),
        (5, "ripple", "software"),
        (6, "peter", "person"),
    ] {
        let node = graph.insert_node(label, [("name".into(), Value::String(name.into()))].into());
        graph.set_element_public_id(&node, Value::Int(id)).unwrap();
        nodes.push(node);
    }
    for (src, dst, label) in [
        (1, 2, "knows"),
        (1, 4, "knows"),
        (1, 3, "created"),
        (4, 3, "created"),
        (4, 5, "created"),
        (6, 3, "created"),
    ] {
        graph
            .insert_edge(label, &nodes[src - 1], &nodes[dst - 1], Default::default())
            .unwrap();
    }
    let mut engine = GraphEngine::in_memory().unwrap();
    engine.replace_graph(graph).unwrap();
    engine
}

#[tokio::test]
async fn limit_preserves_join_outputs_for_id_filtered_sources() {
    let mut engine = modern();
    for (query, count, allowed) in [
        ("g.V(1).out().limit(2)", 2, vec![2, 3, 4]),
        (
            "g.V(1).out('knows').outE('created').range(0,1).inV()",
            1,
            vec![3, 5],
        ),
        (
            "g.V(1).out('knows').out('created').range(0,1)",
            1,
            vec![3, 5],
        ),
        (
            "g.V(1).out('created').in('created').range(1,3)",
            2,
            vec![1, 4, 6],
        ),
        (
            "g.V(1).out('created').inE('created').range(1,3).outV()",
            2,
            vec![1, 4, 6],
        ),
    ] {
        let result = engine
            .gremlin(query)
            .await
            .unwrap_or_else(|e| panic!("{query}: {e}"));
        let rows: serde_json::Value = serde_json::from_str(
            &result.returned.batch.schema().metadata()["orchiddb.gremlin.typed_rows.v1"],
        )
        .unwrap();
        let rows = rows.as_array().unwrap();
        assert_eq!(rows.len(), count, "{query}");
        for row in rows {
            assert!(
                allowed.contains(&row[0]["id"]["value"].as_i64().unwrap()),
                "{query}: {row}"
            );
        }
        assert!(
            !result.stats.sql_queries.is_empty(),
            "{query} must execute a SQL island"
        );
    }
}

#[tokio::test]
async fn local_and_map_limits_keep_correlation_and_endpoint_properties() {
    let mut engine = modern();
    for (query, expected) in [
        (
            "g.V().local(__.outE().limit(1)).inV().limit(3).values('name')",
            3,
        ),
        ("g.V().map(__.in().hasId(1)).limit(2).values('name')", 2),
    ] {
        let result = engine
            .gremlin(query)
            .await
            .unwrap_or_else(|e| panic!("{query}: {e}"));
        assert_eq!(result.returned.batch.num_rows(), expected, "{query}");
        for i in 0..expected {
            let name =
                arrow::util::display::array_value_to_string(result.returned.batch.column(0), i)
                    .unwrap();
            assert!(
                ["marko", "vadas", "josh", "lop", "ripple"].contains(&name.as_str()),
                "{query}: {name}"
            );
            if query.contains("map(") {
                assert_eq!(name, "marko");
            }
        }
    }
}
