#![cfg(feature = "duckdb")]
use orchiddb::{
    engine::GraphEngine,
    ir::rel::mapping::{EdgeMapping, GraphMapping, NodeMapping},
};
use std::sync::Arc;
fn engine() -> GraphEngine {
    let db = duckdb::Connection::open_in_memory().unwrap();
    db.execute_batch("CREATE TABLE vertices(id VARCHAR PRIMARY KEY); INSERT INTO vertices VALUES ('a'),('b'),('c'); CREATE TABLE edges(id VARCHAR PRIMARY KEY,src VARCHAR,dst VARCHAR); INSERT INTO edges VALUES ('ab','a','b'),('ac','a','c'),('ba','b','a'),('ca','c','a')").unwrap();
    let mut mapping = GraphMapping::new();
    mapping.map_node(NodeMapping::table("V", "vertices", "id").property("id", "id"));
    mapping.map_edge(EdgeMapping::table("E", "edges", "src", "dst", "V", "V").with_id("id"));
    GraphEngine::mapped(db, Arc::new(mapping)).unwrap()
}
