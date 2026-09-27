//! Run: cargo run --features duckdb --example composite_keys
//! Composite identities and an FK stored directly on the Order row.
#[cfg(feature = "duckdb")]
#[tokio::main]
async fn main() -> Result<(), String> {
    use orchiddb::{
        engine::GraphEngine,
        ir::rel::mapping::{EdgeMapping, ForeignKeyEndpoint, GraphMapping, NodeMapping},
    };
    use std::sync::Arc;
    let db = duckdb::Connection::open_in_memory().map_err(|e| e.to_string())?;
    db.execute_batch("CREATE TABLE customers(tenant VARCHAR, id BIGINT, name VARCHAR NOT NULL, PRIMARY KEY(tenant,id));
        CREATE TABLE orders(tenant VARCHAR, id BIGINT, customer_id BIGINT, total DOUBLE NOT NULL, PRIMARY KEY(tenant,id), FOREIGN KEY(tenant,customer_id) REFERENCES customers(tenant,id));
        CREATE TABLE warehouses(id BIGINT PRIMARY KEY);
        CREATE TABLE shipments(tenant VARCHAR, order_id BIGINT, warehouse_id BIGINT, PRIMARY KEY(tenant,order_id,warehouse_id), FOREIGN KEY(tenant,order_id) REFERENCES orders(tenant,id), FOREIGN KEY(warehouse_id) REFERENCES warehouses(id));")
        .map_err(|e| e.to_string())?;
    let inspect = db.try_clone().map_err(|e| e.to_string())?;
    let mut mapping = GraphMapping::new();
    mapping.map_node(
        NodeMapping::table("Customer", "customers", ["tenant", "id"])
            .property("tenant", "tenant")
            .property("id", "id")
            .property("name", "name"),
    );
    mapping.map_node(
        NodeMapping::table("Order", "orders", ["tenant", "id"])
            .property("tenant", "tenant")
            .property("id", "id")
            .property("total", "total"),
    );
    mapping.map_node(NodeMapping::table("Warehouse", "warehouses", "id").property("id", "id"));
    mapping.map_edge(
        EdgeMapping::table(
            "HAS_ORDER",
            "orders",
            ["tenant", "customer_id"],
            ["tenant", "id"],
            "Customer",
            "Order",
        )
        .foreign_key(ForeignKeyEndpoint::Destination),
    );
    mapping.map_edge(
        EdgeMapping::table(
            "SHIPS_TO",
            "shipments",
            ["tenant", "order_id"],
            "warehouse_id",
            "Order",
            "Warehouse",
        )
        .with_id(["tenant", "order_id", "warehouse_id"])
        .property("tenant", "tenant")
        .property("order", "order_id")
        .property("warehouse", "warehouse_id"),
    );
    // The same ordered keys survive the public TOML mapping format.
    let mapping = GraphMapping::from_toml(&mapping.to_toml()).map_err(|e| e.to_string())?;
    let mut engine = GraphEngine::mapped(db, Arc::new(mapping))?;
    for query in [
        "CREATE (c:Customer {tenant:'acme',id:42,name:'Alice'})-[:HAS_ORDER]->(:Order {tenant:'acme',id:101,total:29.95}), (:Customer {tenant:'acme',id:43,name:'Bob'}), (:Customer {tenant:'other',id:42,name:'Other Alice'})",
        "MATCH (old:Customer)-[r:HAS_ORDER]->(o:Order), (next:Customer {tenant:'acme',id:43}) DELETE r CREATE (next)-[:HAS_ORDER]->(o) RETURN next.name,o.id",
        "MATCH (o:Order {tenant:'acme',id:101}) CREATE (o)-[:SHIPS_TO {tenant:'acme',order:101,warehouse:7}]->(:Warehouse {id:7})",
        "MATCH (c:Customer)-[:HAS_ORDER]->(o:Order) RETURN c.tenant,c.name,o.total",
        "MATCH (n) RETURN count(n)",
        "MATCH (a)-[:SHIPS_TO]-(b) RETURN count(*)",
        "MATCH p=(c:Customer {tenant:'acme',id:43})-[:HAS_ORDER|SHIPS_TO*1..2]->(n) RETURN length(p)",
    ] {
        let result = engine.cypher(query).await?;
        println!("{query}\n{:?}", result.returned.batch);
    }
    for query in [
        "g.addV('Customer').property(T.id,['acme',44]).property('name','Carol')",
        "g.V().hasLabel('Customer').hasId(eq(['acme',44])).values('name')",
        "g.V().hasLabel('Customer').has('name','Bob').out('HAS_ORDER').id()",
    ] {
        let result = engine.gremlin(query).await?;
        println!("{query}\n{:?}", result.returned.batch);
    }
    let row: (String, i64, i64) = inspect
        .query_row("SELECT tenant,id,customer_id FROM orders", [], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })
        .map_err(|e| e.to_string())?;
    println!("Physical order row: {row:?}");
    println!("Snapshot: {:?}", engine.cypher_state_snapshot()?.nodes);
    // Remove the independent shipment before unlinking its referenced order.
    engine.cypher("MATCH ()-[r:SHIPS_TO]->() DELETE r").await?;
    engine.cypher("MATCH ()-[r:HAS_ORDER]->() DELETE r").await?;
    let remaining: i64 = inspect
        .query_row(
            "SELECT count(*) FROM orders WHERE customer_id IS NULL",
            [],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    println!("Orders preserved after unlink: {remaining}");
    Ok(())
}
#[cfg(not(feature = "duckdb"))]
fn main() {
    eprintln!("Enable the duckdb feature to run this example.");
}
