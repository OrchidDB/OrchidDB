//! Compiler-only composite joins must work without native graph operators.
#![cfg(feature = "duckdb")]
use orchiddb::compiler::compile_json;
use serde_json::{Value, json};

fn request(language: &str, query: &str) -> Value {
    json!({
        "version": 1, "dialect": "duckdb", "language": language, "query": query,
        "tables": [
            {"name":"customers","columns":[{"name":"tenant","data_type":"string"},{"name":"id","data_type":"int64"},{"name":"name","data_type":"string"}]},
            {"name":"orders","columns":[{"name":"tenant","data_type":"string"},{"name":"id","data_type":"int64"},{"name":"customer_id","data_type":"int64"},{"name":"total","data_type":"int64"}]}
        ],
        "nodes": [
            {"label":"Customer","table":"customers","id":["tenant","id"],"properties":{"name":"name"}},
            {"label":"Order","table":"orders","id":["tenant","id"],"properties":{"total":"total"}}
        ],
        "edges": [{"label":"HAS_ORDER","table":"orders","id":["tenant","id"],"source":["tenant","customer_id"],"target":["tenant","id"],"source_label":"Customer","target_label":"Order","foreign_key":"dst"}]
    })
}

async fn run(language: &str, query: &str) -> Vec<String> {
    let compiled: Value = serde_json::from_str(
        &compile_json(&request(language, query).to_string())
            .await
            .unwrap(),
    )
    .unwrap();
    let db = duckdb::Connection::open_in_memory().unwrap();
    db.execute_batch("CREATE TABLE customers(tenant VARCHAR, id BIGINT, name VARCHAR, PRIMARY KEY(tenant,id));
        CREATE TABLE orders(tenant VARCHAR, id BIGINT, customer_id BIGINT, total BIGINT, PRIMARY KEY(tenant,id));
        INSERT INTO customers VALUES ('a',7,'Ada'),('b',7,'Grace');
        INSERT INTO orders VALUES ('a',1,7,10),('b',1,7,20),('a',2,NULL,30);").unwrap();
    let sql = format!(
        "SELECT COLUMNS(*)::VARCHAR FROM ({})",
        compiled["sql"].as_str().unwrap()
    );
    let mut statement = db.prepare(&sql).unwrap_or_else(|e| panic!("{e}\n{sql}"));
    let mut rows = statement.query([]).unwrap();
    let width = rows.as_ref().unwrap().column_count();
    let mut result = Vec::new();
    while let Some(row) = rows.next().unwrap() {
        result.push(
            (0..width)
                .map(|i| row.get::<_, String>(i).unwrap())
                .collect::<Vec<_>>()
                .join("|"),
        );
    }
    result.sort();
    result
}

#[tokio::test]
async fn cypher_composite_fk_joins_keep_tenants_and_skip_partial_nulls() {
    assert_eq!(
        run(
            "cypher",
            "MATCH (c:Customer)-[:HAS_ORDER]->(o:Order) RETURN c.name, o.total"
        )
        .await,
        ["Ada|10", "Grace|20"]
    );
}

#[tokio::test]
async fn gremlin_composite_fk_traversal_keeps_tenants_and_skips_partial_nulls() {
    assert_eq!(
        run(
            "gremlin",
            "g.V().hasLabel('Customer').out('HAS_ORDER').values('total')"
        )
        .await,
        ["10", "20"]
    );
}
