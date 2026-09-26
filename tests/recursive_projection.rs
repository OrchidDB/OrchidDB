//! Recursive work-table columns must keep their positions after optimization.
use arrow::array::{ArrayRef, Int64Array, RecordBatch, StringArray};
use datafusion::datasource::MemTable;
use orchiddb::ir::PropertyGraph;
use orchiddb::ir::rel::RelBackendOptions;
use orchiddb::ir::rel::mapping::{EdgeMapping, GraphMapping, NodeMapping};
use orchiddb::ir::rel::{RelBackend, dag, execute_lowered};
use orchiddb::language::cypher::parser::parse_query;
use orchiddb::language::cypher::planner::CypherPlanner;
use std::sync::Arc;

fn graph(edges: usize, cycle: bool) -> Arc<GraphMapping> {
    let count = if cycle { edges } else { edges + 1 };
    let nodes = RecordBatch::try_from_iter(vec![
        (
            "id",
            Arc::new(Int64Array::from((0..count as i64).collect::<Vec<_>>())) as ArrayRef,
        ),
        (
            "name",
            Arc::new(StringArray::from(
                (0..count).map(|i| format!("n{i}")).collect::<Vec<_>>(),
            )) as ArrayRef,
        ),
        (
            "unused",
            Arc::new(Int64Array::from(vec![42; count])) as ArrayRef,
        ),
    ])
    .unwrap();
    let edges = RecordBatch::try_from_iter(vec![
        (
            "id",
            Arc::new(Int64Array::from((0..edges as i64).collect::<Vec<_>>())) as ArrayRef,
        ),
        (
            "src",
            Arc::new(Int64Array::from((0..edges as i64).collect::<Vec<_>>())) as ArrayRef,
        ),
        (
            "dst",
            Arc::new(Int64Array::from(
                (1..=edges).map(|i| (i % count) as i64).collect::<Vec<_>>(),
            )) as ArrayRef,
        ),
    ])
    .unwrap();
    let mut mapping = GraphMapping::new();
    for (name, batch) in [("nodes", nodes), ("edges", edges)] {
        mapping.register_table(
            name,
            Arc::new(MemTable::try_new(batch.schema(), vec![vec![batch]]).unwrap()),
        );
    }
    mapping.map_node(
        NodeMapping::table("N", "nodes", "id")
            .property("name", "name")
            .property("unused", "unused"),
    );
    mapping.map_edge(EdgeMapping::table("E", "edges", "src", "dst", "N", "N").with_id("id"));
    Arc::new(mapping)
}

async fn check(graph: &Arc<GraphMapping>, query: &str, expected: i64) {
    let plan = CypherPlanner::new()
        .plan(&parse_query(query).unwrap())
        .unwrap();
    for use_dag in [false, true] {
        let lowered = RelBackend::with_options(RelBackendOptions {
            mapping: Some(graph.clone()),
            ..Default::default()
        })
        .lower(&plan, &PropertyGraph::new())
        .unwrap();
        // Keep this a regression for recursion, not the bounded-unroll fallback.
        assert!(
            lowered
                .plan
                .display_indent()
                .to_string()
                .contains("RecursiveQuery")
        );
        let result = if use_dag {
            dag::execute(lowered).await.map(|(result, _)| result)
        } else {
            execute_lowered(lowered).await
        }
        .unwrap_or_else(|e| panic!("dag={use_dag}, {query}: {e}"));
        let batch = result.batch;
        assert_eq!(batch.num_rows(), 1, "{query}");
        assert_eq!(
            batch
                .column(0)
                .as_any()
                .downcast_ref::<Int64Array>()
                .unwrap()
                .value(0),
            expected,
            "dag={use_dag}, {query}"
        );
    }
}

#[tokio::test]
async fn recursive_state_survives_narrow_projections_and_aggregates() {
    let chain = graph(15, false);
    for (query, expected) in [
        ("MATCH (a:N {name:'n0'})-[:E*]->(b:N) RETURN count(*)", 15),
        (
            "MATCH (a:N {name:'n0'})-[:E*1..10]->(b:N) RETURN count(*)",
            10,
        ),
        (
            "MATCH (a:N {name:'missing'})-[:E*]->(b:N) RETURN count(*)",
            0,
        ),
        ("MATCH (a:N {name:'n15'})<-[:E*]-(b:N) RETURN count(*)", 15),
        (
            "MATCH (a:N {name:'n0'})-[:E*]->(b:N) WITH b.name AS name RETURN count(name)",
            15,
        ),
    ] {
        check(&chain, query, expected).await;
    }
}

#[tokio::test]
async fn recursive_cycles_terminate_without_reusing_edges() {
    let cycle = graph(3, true);
    check(
        &cycle,
        "MATCH (a:N {name:'n0'})-[:E*]->(b:N) RETURN count(*)",
        3,
    )
    .await;
    check(
        &cycle,
        "MATCH (a:N {name:'n0'})-[:E*0..]->(b:N) RETURN count(*)",
        4,
    )
    .await;
}
