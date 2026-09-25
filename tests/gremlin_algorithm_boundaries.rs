//! Native execution must not fabricate GraphComputer results.
use std::collections::BTreeMap;

use orchiddb::ir::catalog::{Cardinality, PropertyGraph};
use orchiddb::ir::interpreter::execute_rows;
use orchiddb::ir::value::Value;
use orchiddb::language::gremlin::{GremlinPlanner, parse_traversal};

fn values(graph: &PropertyGraph, query: &str) -> Vec<Value> {
    let traversal = parse_traversal(query).unwrap();
    let plan = GremlinPlanner::new().plan(&traversal).unwrap();
    execute_rows(&plan, graph)
        .unwrap()
        .into_iter()
        .map(|row| row.bindings["current"].clone())
        .collect()
}

#[test]
fn native_graph_computer_steps_require_the_computer_profile() {
    for step in [
        "pageRank()",
        "pageRank(0.5)",
        "peerPressure()",
        "connectedComponent()",
        "shortestPath()",
        "shortestPath().with('~tinkerpop.shortestPath.distance','weight')",
    ] {
        let query = format!("g.V().{step}");
        let traversal = parse_traversal(&query).unwrap();
        let error = GremlinPlanner::new().plan(&traversal).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("JVM GraphComputer execution profile"),
            "{query}: {error}"
        );
    }
}

#[test]
fn algorithm_properties_exist_only_when_stored_for_every_vertex_name() {
    let keys = [
        "gremlin.connectedComponentVertexProgram.component",
        "component",
        "gremlin.peerPressureVertexProgram.cluster",
        "cluster",
        "gremlin.pageRankVertexProgram.pageRank",
        "pageRank",
        "projectRank",
        "priors",
        "friendRank",
        "rank",
    ];
    // Reordered and unrelated vertex names must obey the same property rules.
    for name in ["ripple", "unknown", "marko", "lop", "renamed"] {
        let graph = PropertyGraph::new();
        let vertex = graph.insert_node(
            "arbitrary",
            BTreeMap::from([("name".into(), Value::String(name.into()))]),
        );
        for key in keys {
            for query in [
                format!("g.V().properties('{key}')"),
                format!("g.V().values('{key}')"),
                format!("g.V().has('{key}',0.15)"),
                format!("g.V().has('{key}')"),
            ] {
                assert!(values(&graph, &query).is_empty(), "{name}: {query}");
            }
            assert_eq!(
                values(&graph, &format!("g.V().valueMap('{key}')")),
                vec![Value::Map(BTreeMap::new())],
                "{name}: {key}"
            );
            graph
                .set_vertex_property(
                    &vertex,
                    key,
                    Value::Float(42.25),
                    Cardinality::Single,
                    BTreeMap::new(),
                )
                .unwrap();
            assert_eq!(
                values(&graph, &format!("g.V().values('{key}')")),
                vec![Value::Float(42.25)],
                "stored {name}: {key}"
            );
            assert!(matches!(
                values(&graph, &format!("g.V().properties('{key}')")).as_slice(),
                [Value::VertexProperty { value, .. }] if value.as_ref() == &Value::Float(42.25)
            ));
        }
    }
}
