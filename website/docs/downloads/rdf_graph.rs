use std::sync::Arc;
use arrow::datatypes::{DataType, Field, Schema};
use orchiddb::engine::{GraphEngine, SparqlResults};
use orchiddb::ir::rel::mapping::{GraphMapping, NodeMapping};
use orchiddb::ir::rel::rdf_mapping::{RdfMapping, RdfTermMapping as Term};

#[tokio::main]
async fn main() -> Result<(), String> {
    let connection = duckdb::Connection::open_in_memory().map_err(|e| e.to_string())?;
    connection.execute_batch(
        "CREATE TABLE customers(tenant BIGINT, id BIGINT, name VARCHAR NOT NULL, PRIMARY KEY(tenant,id));
         INSERT INTO customers VALUES (1,7,'Alice');"
    ).map_err(|e| e.to_string())?;
    let mut mapping = GraphMapping::new();
    mapping.register_table_schema("customers", Arc::new(Schema::new(vec![
        Field::new("tenant", DataType::Int64, false),
        Field::new("id", DataType::Int64, false),
        Field::new("name", DataType::Utf8, false),
    ])));
    mapping.map_node(NodeMapping::table("Customer", "customers", ["tenant", "id"])
        .property("name", "name"));
    mapping.map_rdf(RdfMapping::table(
        "customers", Term::template("urn:customer:", ["tenant", "id"]),
        "urn:name", Term::literal("name"),
    ).writable(["tenant", "id"]));
    let mut graph = GraphEngine::mapped(connection, Arc::new(mapping))?;
    let query = "SELECT ?name WHERE { ?customer <urn:name> ?name }";
    if let SparqlResults::Solutions { variables, rows } = graph.sparql_query(query, "default").await? {
        println!("{variables:?}: {rows:?}");
    }
    graph.begin()?;
    let update = graph.sparql_update(
        "DELETE { ?customer <urn:name> ?old } INSERT { ?customer <urn:name> \"Alicia\" }
         WHERE { ?customer <urn:name> ?old FILTER(?old = \"Alice\") }", "default", None,
    ).await;
    if let Err(error) = update {
        graph.rollback()?;
        return Err(error);
    }
    let result = graph.cypher("MATCH (c:Customer) RETURN c.name").await?;
    assert_eq!(arrow::util::display::array_value_to_string(result.returned.batch.column(0),0)
        .map_err(|e|e.to_string())?, "Alicia");
    graph.rollback()?;
    let result = graph.sparql_dataset(query, "default").await?;
    println!("{}", result.stats.logical_plan);
    assert_eq!(arrow::util::display::array_value_to_string(result.returned.batch.column(0),0)
        .map_err(|e|e.to_string())?, "Alice");
    Ok(())
}
