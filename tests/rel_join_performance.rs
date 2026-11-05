#![cfg(feature = "duckdb")]

use std::sync::Arc;

use arrow::array::{ArrayRef, Int64Array, StringArray};
use orchiddb::ir::catalog::{PropertyGraph, edges_from_columns, nodes_from_columns};
use orchiddb::ir::rel::{RelBackend, sql};
use orchiddb::language::cypher::{parser::parse_query, planner::CypherPlanner};

fn fixture() -> PropertyGraph {
    let mut graph = PropertyGraph::new();
    graph.add_nodes(nodes_from_columns(
        "P",
        vec![(
            "name",
            Arc::new(StringArray::from(vec!["a", "b"])) as ArrayRef,
        )],
    ));
    graph.add_nodes(nodes_from_columns(
        "Q",
        vec![("name", Arc::new(StringArray::from(vec!["q"])) as ArrayRef)],
    ));
    graph
        .add_edges(edges_from_columns(
            "R",
            "P",
            "P",
            vec![0, 0, 0],
            vec![0, 1, 1],
            vec![(
                "weight",
                Arc::new(Int64Array::from(vec![10, 20, 30])) as ArrayRef,
            )],
        ))
        .unwrap();
    // The endpoint IDs coincide but the labels differ: this is not a loop.
    graph
        .add_edges(edges_from_columns(
            "R",
            "P",
            "Q",
            vec![0],
            vec![0],
            vec![("weight", Arc::new(Int64Array::from(vec![40])) as ArrayRef)],
        ))
        .unwrap();
    graph
}

async fn rows(query: &str) -> Vec<String> {
    let graph = fixture();
    let plan = CypherPlanner::new()
        .plan(&parse_query(query).unwrap())
        .unwrap();
    let lowered = RelBackend::new().lower(&plan, &graph).unwrap();
    let prepared = sql::prepare(&lowered, sql::SqlDialect::DuckDb)
        .await
        .unwrap();
    if std::env::var("SHOW_JOIN_SQL").is_ok() {
        eprintln!("{query}\n{}", prepared.query);
    }
    let result = sql::execute_prepared(&mut sql::DuckDbExecutor::new(), &prepared).unwrap();
    let mut rows = (0..result.batch.num_rows())
        .map(|row| {
            result
                .batch
                .columns()
                .iter()
                .map(|column| arrow::util::display::array_value_to_string(column, row).unwrap())
                .collect::<Vec<_>>()
                .join("|")
        })
        .collect::<Vec<_>>();
    rows.sort();
    rows
}

#[tokio::test]
async fn undirected_edges_preserve_loops_parallel_edges_and_endpoint_labels() {
    assert_eq!(
        rows("MATCH (a)-[r:R]-(b) RETURN a.name, b.name, r.weight").await,
        [
            "a|a|10", "a|b|20", "a|b|30", "a|q|40", "b|a|20", "b|a|30", "q|a|40"
        ],
    );
    assert_eq!(
        rows("MATCH (a:P), (b:P) MATCH (a)-[r:R]-(b) RETURN a.name, b.name, r.weight").await,
        ["a|a|10", "a|b|20", "a|b|30", "b|a|20", "b|a|30"],
    );
}

#[tokio::test]
async fn optional_joins_preserve_duplicate_and_null_correlations() {
    assert_eq!(
        rows("UNWIND [1, 1, null] AS x MATCH (a:P {name: 'a'}) OPTIONAL MATCH (a)-[:R]->(b:P) OPTIONAL MATCH (b)-[:R]->(c:P) RETURN count(*)").await,
        ["15"],
    );
    assert_eq!(
        rows("UNWIND [1, null, 2] AS x RETURN count(*)").await,
        ["3"]
    );
    assert_eq!(rows("MATCH (p:P), (q:Q) RETURN count(*)").await, ["2"]);
    assert_eq!(
        rows("MATCH (p:P) WHERE p.name = 'missing' RETURN count(*)").await,
        ["0"]
    );
}

#[tokio::test]
async fn single_hop_existence_preserves_outer_multiplicity() {
    assert_eq!(rows("UNWIND [1, 1, null] AS x MATCH (a:P), (b:Q) WHERE EXISTS { MATCH (a)-[:R]-(b) } RETURN count(*)").await, ["3"]);
    assert_eq!(rows("UNWIND [1, 1, null] AS x MATCH (a:P), (b:Q) WHERE NOT EXISTS { MATCH (a)-[:R]-(b) } RETURN count(*)").await, ["3"]);
    assert_eq!(
        rows("MATCH (a:P {name: 'b'}) OPTIONAL MATCH (a)-[:R]->(b:Q) RETURN a.name, b.name").await,
        ["b|"]
    );
}

#[tokio::test]
async fn existence_can_filter_before_an_unrelated_expansion() {
    assert_eq!(rows("MATCH (a:P)-[:R]-(b:P)-[:R]->(q:Q) WHERE NOT EXISTS { MATCH (a)-[:R]-(b) } RETURN count(*)").await, ["0"]);
    assert_eq!(
        rows(
            "MATCH (a:P)-[:R]-(b:P)-[:R]->(q:Q) WHERE EXISTS { MATCH (a)-[:R]-(b) } RETURN count(*)"
        )
        .await,
        ["3"]
    );
    assert_eq!(rows("MATCH (a:P), (b:P), (c:P)-[:R]->(q:Q) WHERE NOT EXISTS { MATCH (a)-[:R]-(b) } RETURN count(*)").await, ["1"]);
    assert_eq!(rows("MATCH (a:P), (b:P), (c:P)-[:R]->(q:Q) WHERE EXISTS { MATCH (a)-[:R]-(b) } RETURN count(*)").await, ["3"]);
}

#[tokio::test]
async fn mapped_filtered_optional_matches_keep_duplicate_and_null_occurrences() {
    use serde_json::json;
    let people=[(1_i64,"Ada",37_i64),(2,"Bob",21),(4,"Bob",21),(42,"Ada",37)];
    let edges=[(10_i64,1_i64,2_i64,5_i64),(11,1,2,7),(14,2,2,1),(15,4,1,0)];
    let connection=duckdb::Connection::open_in_memory().unwrap();
    connection.execute_batch("CREATE TABLE people(id BIGINT,name VARCHAR,age BIGINT); CREATE TABLE links(id BIGINT,src BIGINT,dst BIGINT,weight BIGINT)").unwrap();
    for (id,name,age) in people {
        connection.execute("INSERT INTO people VALUES (?,?,?)",duckdb::params![id,name,age]).unwrap();
    }
    for (id,src,dst,weight) in edges {
        connection.execute("INSERT INTO links VALUES (?,?,?,?)",duckdb::params![id,src,dst,weight]).unwrap();
    }
    for name in ["Ada","Bob"] {
    for alias in [false,true] {
        let prefix=if alias {format!("MATCH (original:Person) WHERE original.name='{name}' WITH original AS n")}
            else {format!("MATCH (n:Person) WHERE n.name='{name}'")};
        let query=format!("{prefix} UNWIND [1,1,null] AS occurrence OPTIONAL MATCH (n)-[r:LINK]->(target:Person) WHERE target.age>=21 RETURN n.id AS source,target.id AS target,r.weight AS weight,occurrence");
        let request=json!({"version":1,"language":"cypher","native_values":false,"dialect":"duckdb",
            "sources":{"db":{"adapter":"duckdb"}},"query":query,
            "tables":[{"name":"people","source":"db","columns":[
                {"name":"id","data_type":"int64"},{"name":"name","data_type":"string"},{"name":"age","data_type":"int64"}]},
                {"name":"links","source":"db","columns":[{"name":"id","data_type":"int64"},
                    {"name":"src","data_type":"int64"},{"name":"dst","data_type":"int64"},{"name":"weight","data_type":"int64"}]}],
            "nodes":[{"label":"Person","table":"people","id":"id","properties":{"id":"id","name":"name","age":"age"}}],
            "edges":[{"label":"LINK","table":"links","id":"id","source_label":"Person","target_label":"Person",
                "source":"src","target":"dst","properties":{"weight":"weight"}}]});
        let compiled:serde_json::Value=serde_json::from_str(&orchiddb::compiler::compile_json(&request.to_string()).await.unwrap()).unwrap();
        let nodes=compiled["execution_plan"]["nodes"].as_array().unwrap();
        assert_eq!(nodes.len(),1,"{compiled}");
        let sql=nodes[0]["sql"].as_str().unwrap();
        let mut statement=connection.prepare(sql).unwrap();
        let mut actual=statement.query_map([],|row|Ok((row.get::<_,i64>(0)?,row.get::<_,Option<i64>>(1)?,
            row.get::<_,Option<i64>>(2)?,row.get::<_,Option<i64>>(3)?))).unwrap()
            .collect::<Result<Vec<_>,_>>().unwrap();
        let mut expected=Vec::new();
        for (source,source_name,_) in people {
            if source_name!=name {continue;}
            let targets=edges.iter().filter(|(_,start,end,_)|*start==source && people.iter().any(|(id,_,age)|id==end && *age>=21))
                .map(|(_,_,target,weight)|(Some(*target),Some(*weight))).collect::<Vec<_>>();
            for occurrence in [Some(1_i64),Some(1),None] {
                for (target,weight) in if targets.is_empty() {vec![(None,None)]}else{targets.clone()} {
                    expected.push((source,target,weight,occurrence));
                }
            }
        }
        actual.sort();expected.sort();assert_eq!(actual,expected,"name={name}, alias={alias}; {sql}");
    }
    }
}
