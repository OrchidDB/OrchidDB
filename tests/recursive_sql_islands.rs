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


async fn rows(query: &str) -> Vec<Vec<String>> {
    let result = engine().cypher(query).await.unwrap();
    let batch = &result.returned.batch;
    (0..batch.num_rows()).map(|row| batch.columns().iter().map(|column| {
        if column.is_null(row) { "<NULL>".into() }
        else { arrow::util::display::array_value_to_string(column, row).unwrap() }
    }).collect()).collect()
}

#[tokio::test]
async fn recursive_trails_preserve_distinct_paths_and_zero_hop_identity() {
    let actual = rows("MATCH p=(a:V {id:'a'})-[:E*0..3]->(b:V) RETURN length(p),b.id ORDER BY length(p),b.id").await;
    let expected = [("0","a"),("1","b"),("1","c"),("2","a"),("2","a"),("3","b"),("3","c")]
        .into_iter().map(|(n,id)| vec![n.to_owned(),id.to_owned()]).collect::<Vec<_>>();
    assert_eq!(actual, expected);
}

#[tokio::test]
async fn recursive_paths_respect_an_already_bound_destination() {
    let actual = rows("MATCH (a:V {id:'a'}),(b:V {id:'b'}) MATCH p=(a)-[:E*1..3]->(b) RETURN length(p) ORDER BY length(p)").await;
    assert_eq!(actual, vec![vec!["1".to_owned()],vec!["3".to_owned()]]);
}

#[tokio::test]
async fn optional_recursive_match_preserves_unmatched_input() {
    let actual = rows("MATCH (a:V {id:'a'}) OPTIONAL MATCH (a)-[:E*1..3]->(b:V {id:'missing'}) RETURN a.id,b.id").await;
    assert_eq!(actual, vec![vec!["a".to_owned(),"<NULL>".to_owned()]]);
}
