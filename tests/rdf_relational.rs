#![cfg(feature = "duckdb")]
use arrow::datatypes::{DataType, Field, Schema};
use orchiddb::{
    engine::GraphEngine,
    ir::rel::{
        mapping::{GraphMapping, NodeMapping, schema_only_provider},
        rdf_mapping::{RdfMapping, RdfTermMapping as T},
    },
    rdf_engine::{RdfTermValue, SparqlResults},
};
use std::sync::Arc;
fn setup() -> GraphEngine {
    let connection = duckdb::Connection::open_in_memory().unwrap();
    connection.execute_batch("CREATE TABLE customers(tenant BIGINT, id BIGINT, name VARCHAR NOT NULL, PRIMARY KEY(tenant,id)); CREATE TABLE orders(tenant BIGINT,id BIGINT,customer_id BIGINT,total BIGINT,PRIMARY KEY(tenant,id)); INSERT INTO customers VALUES(1,7,'Alice'),(2,7,'Bob'); INSERT INTO orders VALUES(1,3,7,150),(2,3,7,200),(1,4,NULL,999)").unwrap();
    let mut map = GraphMapping::new();
    for (table, columns) in [
        (
            "customers",
            vec![
                ("tenant", DataType::Int64),
                ("id", DataType::Int64),
                ("name", DataType::Utf8),
            ],
        ),
        (
            "orders",
            vec![
                ("tenant", DataType::Int64),
                ("id", DataType::Int64),
                ("customer_id", DataType::Int64),
                ("total", DataType::Int64),
            ],
        ),
    ] {
        map.register_table(
            table,
            schema_only_provider(Arc::new(Schema::new(
                columns
                    .into_iter()
                    .map(|(n, t)| Field::new(n, t, true))
                    .collect::<Vec<_>>(),
            ))),
        );
    }
    map.map_node(
        NodeMapping::table("Customer", "customers", ["tenant", "id"]).property("name", "name"),
    );
    map.map_rdf(
        RdfMapping::table(
            "customers",
            T::template("urn:c:", ["tenant", "id"]),
            "urn:name",
            T::literal("name"),
        )
        .writable(["tenant", "id"]),
    );
    map.map_rdf(
        RdfMapping::table(
            "orders",
            T::template("urn:c:", ["tenant", "customer_id"]),
            "urn:hasOrder",
            T::template("urn:o:", ["tenant", "id"]),
        )
        .writable(["tenant", "id"]),
    );
    map.map_rdf(
        RdfMapping::table(
            "orders",
            T::template("urn:o:", ["tenant", "id"]),
            "urn:total",
            T::literal("total"),
        )
        .writable(["tenant", "id"]),
    );
    GraphEngine::mapped(connection, Arc::new(map)).unwrap()
}
#[tokio::test]
async fn joins_composite_application_rows_and_prunes_predicates() {
    let mut engine = setup();
    let query = "SELECT ?name ?total WHERE {?c <urn:name> ?name; <urn:hasOrder> ?o. ?o <urn:total> ?total FILTER(?total>100)} ORDER BY ?name";
    let SparqlResults::Solutions { rows, .. } =
        engine.sparql_query(query, "default").await.unwrap()
    else {
        panic!()
    };
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0][0], Some(RdfTermValue::string("Alice")));
    assert_eq!(rows[1][0], Some(RdfTermValue::string("Bob")));
    let sql = engine
        .sparql_sql("SELECT ?n WHERE {?c <urn:name> ?n}", "default")
        .await
        .unwrap();
    assert!(sql.contains("customers"), "{sql}");
    assert!(!sql.contains("orders"), "{sql}");
    assert_eq!(
        engine
            .sparql_query("ASK {<urn:c:31/37> <urn:name> 'Alice'}", "default")
            .await
            .unwrap(),
        SparqlResults::Boolean(true)
    );
    assert_eq!(
        engine
            .sparql_query("ASK {?s <urn:missing> ?o}", "default")
            .await
            .unwrap(),
        SparqlResults::Boolean(false)
    );
}
#[tokio::test]
async fn writes_share_transactions_and_application_rows() {
    let mut engine = setup();
    engine.begin().unwrap();
    engine.sparql_update("DELETE {<urn:c:31/37> <urn:name> ?n} INSERT {<urn:c:31/37> <urn:name> 'Ann'} WHERE {<urn:c:31/37> <urn:name> ?n}","default",None).await.unwrap();
    let rows = engine
        .cypher("MATCH (c:Customer) WHERE c.name = 'Ann' RETURN c.name")
        .await
        .unwrap();
    assert_eq!(rows.returned.batch.num_rows(), 1);
    engine.rollback().unwrap();
    assert_eq!(
        engine
            .sparql_query("ASK {<urn:c:31/37> <urn:name> 'Alice'}", "default")
            .await
            .unwrap(),
        SparqlResults::Boolean(true)
    );
    engine
        .sparql_update(
            "INSERT DATA {<urn:c:33/38> <urn:name> 'Cara'}",
            "default",
            None,
        )
        .await
        .unwrap();
    let mut executor = engine.into_executor();
    let conn = executor.connection().unwrap();
    let count: i64 = conn
        .query_row(
            "SELECT count(*) FROM customers WHERE tenant=3 AND id=8 AND name='Cara'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, 1);
    let tables: i64 = conn
        .query_row(
            "SELECT count(*) FROM information_schema.tables WHERE table_schema='main'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(tables, 2);
}

#[tokio::test]
async fn relational_algebra_paths_and_inverse_fk_updates() {
    let mut engine = setup();
    for (query, expected) in [
        ("SELECT ?s ?p ?o WHERE {?s ?p ?o}", 7),
        (
            "SELECT ?s ?total WHERE {?s <urn:name> ?n OPTIONAL {?s <urn:hasOrder>/<urn:total> ?total}}",
            2,
        ),
        (
            "SELECT ?s WHERE {{?s <urn:name> 'Alice'} UNION {?s <urn:name> 'Bob'}}",
            2,
        ),
        (
            "SELECT ?s WHERE {?s <urn:name> ?n FILTER EXISTS {?s <urn:hasOrder> ?o}}",
            2,
        ),
        ("SELECT (COUNT(?s) AS ?n) WHERE {?s <urn:name> ?name}", 1),
    ] {
        let SparqlResults::Solutions { rows, .. } =
            engine.sparql_query(query, "default").await.unwrap()
        else {
            panic!()
        };
        assert_eq!(rows.len(), expected, "{query}");
    }
    engine
        .sparql_update(
            "DELETE DATA {<urn:c:31/37> <urn:hasOrder> <urn:o:31/33>}",
            "default",
            None,
        )
        .await
        .unwrap();
    assert_eq!(
        engine
            .sparql_query("ASK {<urn:o:31/33> <urn:total> ?t}", "default")
            .await
            .unwrap(),
        SparqlResults::Boolean(true)
    );
    assert_eq!(
        engine
            .sparql_query("ASK {<urn:c:31/37> <urn:hasOrder> ?o}", "default")
            .await
            .unwrap(),
        SparqlResults::Boolean(false)
    );
    engine
        .sparql_update(
            "INSERT DATA {<urn:c:31/37> <urn:hasOrder> <urn:o:31/33>}",
            "default",
            None,
        )
        .await
        .unwrap();
    assert!(
        engine
            .sparql_update(
                "INSERT DATA {<urn:c:31/37> <urn:name> 'Conflicting'}",
                "default",
                None
            )
            .await
            .is_err()
    );
    assert_eq!(
        engine
            .sparql_query("ASK {<urn:c:31/37> <urn:name> 'Alice'}", "default")
            .await
            .unwrap(),
        SparqlResults::Boolean(true)
    );
    engine.begin().unwrap();
    assert!(
        engine
            .sparql_update(
                "INSERT DATA {<urn:c:31/37> <urn:unknown> 'x'}",
                "default",
                None
            )
            .await
            .is_err()
    );
    assert!(engine.commit().is_err());
    engine.rollback().unwrap();
}

#[tokio::test]
async fn ontology_adapter_does_not_require_type_roots() {
    use orchiddb::language::sparql::OntologyMapping;
    let mut engine = setup();
    let result = engine
        .sparql(
            "SELECT ?name WHERE {?s <urn:legacyName> ?name}",
            OntologyMapping::new()
                .class("urn:Customer", "Customer")
                .property("urn:legacyName", "Customer", "name"),
        )
        .await
        .unwrap();
    assert_eq!(result.returned.batch.num_rows(), 2);
    let result = engine
        .sparql("SELECT (42 AS ?answer) WHERE {}", OntologyMapping::new())
        .await
        .unwrap();
    assert_eq!(
        result
            .returned
            .batch
            .column(0)
            .as_any()
            .downcast_ref::<arrow::array::Int64Array>()
            .unwrap()
            .value(0),
        42
    );
    let result = engine
        .sparql("SELECT * WHERE {}", OntologyMapping::new())
        .await
        .unwrap();
    assert_eq!(result.returned.batch.num_rows(), 1);
    assert!(result.returned.fields.is_empty());
}

#[tokio::test]
async fn named_graphs_language_and_views_use_application_tables() {
    let conn = duckdb::Connection::open_in_memory().unwrap();
    conn.execute_batch("CREATE TABLE labels(id VARCHAR, value VARCHAR, lang VARCHAR, graph VARCHAR); INSERT INTO labels VALUES ('urn:a','Hello','EN','urn:g'),('urn:b','Salut','FR','urn:g'),('urn:hidden','No graph','en',NULL)").unwrap();
    let mut map = GraphMapping::new();
    map.register_table(
        "labels",
        schema_only_provider(Arc::new(Schema::new(
            ["id", "value", "lang", "graph"]
                .map(|c| Field::new(c, DataType::Utf8, true))
                .to_vec(),
        ))),
    );
    map.register_view(
        "visible_labels",
        "SELECT * FROM labels WHERE value <> 'hidden'",
    )
    .unwrap();
    map.map_rdf(
        RdfMapping::table(
            "visible_labels",
            T::iri("id"),
            "urn:label",
            T::Literal {
                column: "value".into(),
                datatype: None,
                language: None,
                language_column: Some("lang".into()),
            },
        )
        .graph(T::iri("graph")),
    );
    let mut engine = GraphEngine::mapped(conn, Arc::new(map)).unwrap();
    let SparqlResults::Solutions { rows, .. } = engine
        .sparql_query(
            "SELECT ?s ?label WHERE {GRAPH <urn:g> {?s <urn:label> ?label}} ORDER BY ?s",
            "default",
        )
        .await
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0][1], Some(RdfTermValue::lang("Hello", "en")));
    assert_eq!(
        engine
            .sparql_query("ASK {?s ?p ?o}", "default")
            .await
            .unwrap(),
        SparqlResults::Boolean(false)
    );
    assert_eq!(
        engine
            .sparql_query("ASK FROM <urn:g> {?s <urn:label> 'Hello'@en}", "default")
            .await
            .unwrap(),
        SparqlResults::Boolean(true)
    );
}

#[tokio::test]
async fn coalesces_required_columns_and_preserves_template_encoding() {
    let conn = duckdb::Connection::open_in_memory().unwrap();
    conn.execute_batch("CREATE TABLE records(id VARCHAR PRIMARY KEY, name VARCHAR NOT NULL, age BIGINT NOT NULL, active BOOLEAN DEFAULT true)").unwrap();
    let mut map = GraphMapping::new();
    map.register_table(
        "records",
        schema_only_provider(Arc::new(Schema::new(vec![
            Field::new("id", DataType::Utf8, false),
            Field::new("name", DataType::Utf8, false),
            Field::new("age", DataType::Int64, false),
            Field::new("active", DataType::Boolean, false),
        ]))),
    );
    for (predicate, column) in [("urn:name", "name"), ("urn:age", "age")] {
        map.map_rdf(
            RdfMapping::table(
                "records",
                T::template("urn:r:", ["id"]),
                predicate,
                T::literal(column),
            )
            .writable(["id"]),
        );
    }
    let mut engine = GraphEngine::mapped(conn, Arc::new(map)).unwrap();
    // z/é has separator and multibyte UTF-8, all escaped in the identity.
    engine
        .sparql_update(
            "INSERT DATA {<urn:r:7a2fc3a9> <urn:name> 'Zed'; <urn:age> 42}",
            "default",
            None,
        )
        .await
        .unwrap();
    assert_eq!(
        engine
            .sparql_query(
                "ASK {<urn:r:7a2fc3a9> <urn:name> 'Zed'; <urn:age> 42}",
                "default"
            )
            .await
            .unwrap(),
        SparqlResults::Boolean(true)
    );
    engine.sparql_update("DELETE {<urn:r:7a2fc3a9> <urn:name> ?name; <urn:age> ?age} INSERT {<urn:r:7a2fc3a9> <urn:name> 'Z'; <urn:age> 43} WHERE {<urn:r:7a2fc3a9> <urn:name> ?name; <urn:age> ?age}","default",None).await.unwrap();
    assert!(
        engine
            .sparql_update(
                "INSERT DATA {<urn:r:61> <urn:name> 'Incomplete'}",
                "default",
                None
            )
            .await
            .is_err()
    );
    let mut executor = engine.into_executor();
    let row: (String, i64, bool) = executor
        .connection()
        .unwrap()
        .query_row(
            "SELECT name,age,active FROM records WHERE id='z/é'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(row, ("Z".into(), 43, true));
}

#[tokio::test]
async fn inserts_follow_composite_foreign_key_dependencies() {
    let conn = duckdb::Connection::open_in_memory().unwrap();
    conn.execute_batch("CREATE TABLE parents(tenant BIGINT,id BIGINT,name VARCHAR NOT NULL,PRIMARY KEY(tenant,id)); CREATE TABLE children(tenant BIGINT,id BIGINT,parent_id BIGINT,name VARCHAR NOT NULL,PRIMARY KEY(tenant,id),FOREIGN KEY(tenant,parent_id) REFERENCES parents(tenant,id))").unwrap();
    let mut map = GraphMapping::new();
    for table in ["parents", "children"] {
        let mut fields = vec![
            Field::new("tenant", DataType::Int64, false),
            Field::new("id", DataType::Int64, false),
            Field::new("name", DataType::Utf8, false),
        ];
        if table == "children" {
            fields.push(Field::new("parent_id", DataType::Int64, true));
        }
        map.register_table(table, schema_only_provider(Arc::new(Schema::new(fields))));
        map.map_rdf(
            RdfMapping::table(
                table,
                T::template(format!("urn:{table}:"), ["tenant", "id"]),
                "urn:name",
                T::literal("name"),
            )
            .writable(["tenant", "id"]),
        );
    }
    map.map_rdf(
        RdfMapping::table(
            "children",
            T::template("urn:parents:", ["tenant", "parent_id"]),
            "urn:child",
            T::template("urn:children:", ["tenant", "id"]),
        )
        .writable(["tenant", "id"]),
    );
    let mut engine = GraphEngine::mapped(conn, Arc::new(map)).unwrap();
    engine.sparql_update("INSERT DATA {<urn:children:31/32> <urn:name> 'Child'. <urn:parents:31/31> <urn:child> <urn:children:31/32>; <urn:name> 'Parent'}","default",None).await.unwrap();
    assert_eq!(
        engine
            .sparql_query(
                "ASK {?p <urn:name> 'Parent'; <urn:child> ?c. ?c <urn:name> 'Child'}",
                "default"
            )
            .await
            .unwrap(),
        SparqlResults::Boolean(true)
    );
}

#[tokio::test]
async fn binary_keys_round_trip_without_utf8_coercion() {
    let conn = duckdb::Connection::open_in_memory().unwrap();
    conn.execute_batch("CREATE TABLE blobs(id BLOB PRIMARY KEY,name VARCHAR); INSERT INTO blobs VALUES(from_hex('ff00'),'Original')").unwrap();
    let mut map = GraphMapping::new();
    map.register_table(
        "blobs",
        schema_only_provider(Arc::new(Schema::new(vec![
            Field::new("id", DataType::Binary, false),
            Field::new("name", DataType::Utf8, true),
        ]))),
    );
    map.map_rdf(
        RdfMapping::table(
            "blobs",
            T::template("urn:blob:", ["id"]),
            "urn:name",
            T::literal("name"),
        )
        .writable(["id"]),
    );
    conn.execute_batch("BEGIN TRANSACTION").unwrap();
    let mut engine = GraphEngine::mapped(conn, Arc::new(map)).unwrap();
    assert!(engine.in_transaction());
    assert_eq!(
        engine
            .sparql_query("ASK {<urn:blob:ff00> <urn:name> 'Original'}", "default")
            .await
            .unwrap(),
        SparqlResults::Boolean(true)
    );
    engine
        .sparql_update(
            "INSERT DATA {<urn:blob:feff> <urn:name> 'New'}",
            "default",
            None,
        )
        .await
        .unwrap();
    assert_eq!(
        engine
            .sparql_query("ASK {<urn:blob:feff> <urn:name> 'New'}", "default")
            .await
            .unwrap(),
        SparqlResults::Boolean(true)
    );
    engine.rollback().unwrap();
    assert_eq!(
        engine
            .sparql_query("ASK {<urn:blob:feff> <urn:name> 'New'}", "default")
            .await
            .unwrap(),
        SparqlResults::Boolean(false)
    );
}

#[tokio::test]
async fn graph_registry_survives_clearing_relational_statements() {
    use orchiddb::ir::rel::rdf::RdfDatasetMapping;
    let conn = duckdb::Connection::open_in_memory().unwrap();
    conn.execute_batch("CREATE TABLE labels(id VARCHAR PRIMARY KEY,name VARCHAR);CREATE TABLE graphs(iri VARCHAR PRIMARY KEY)").unwrap();
    let mut rdf = RdfDatasetMapping::new();
    rdf.register_table(
        "graphs",
        schema_only_provider(Arc::new(Schema::new(vec![Field::new(
            "iri",
            DataType::Utf8,
            false,
        )]))),
    )
    .map_writable_named_graphs("default", "graphs", "iri");
    let mut map = GraphMapping::new().with_rdf_mapping(rdf);
    map.register_table(
        "labels",
        schema_only_provider(Arc::new(Schema::new(vec![
            Field::new("id", DataType::Utf8, false),
            Field::new("name", DataType::Utf8, true),
        ]))),
    );
    map.map_rdf(
        RdfMapping::table("labels", T::iri("id"), "urn:name", T::literal("name"))
            .graph(T::constant("urn:g"))
            .writable(["id"]),
    );
    let mut engine = GraphEngine::mapped(conn, Arc::new(map)).unwrap();
    engine
        .sparql_update(
            "INSERT DATA {GRAPH <urn:g> {<urn:s> <urn:name> 'Name'}}",
            "default",
            None,
        )
        .await
        .unwrap();
    engine
        .sparql_update("CLEAR GRAPH <urn:g>", "default", None)
        .await
        .unwrap();
    assert_eq!(
        engine
            .sparql_query("ASK {GRAPH <urn:g> {}}", "default")
            .await
            .unwrap(),
        SparqlResults::Boolean(true)
    );
    assert_eq!(
        engine
            .sparql_query("ASK {GRAPH <urn:g> {?s ?p ?o}}", "default")
            .await
            .unwrap(),
        SparqlResults::Boolean(false)
    );
    engine
        .sparql_update("DROP GRAPH <urn:g>", "default", None)
        .await
        .unwrap();
    assert_eq!(
        engine
            .sparql_query("ASK {GRAPH <urn:g> {}}", "default")
            .await
            .unwrap(),
        SparqlResults::Boolean(false)
    );
}

#[tokio::test]
async fn a_graph_registry_can_define_an_empty_dataset() {
    use orchiddb::ir::rel::rdf::RdfDatasetMapping;
    let conn = duckdb::Connection::open_in_memory().unwrap();
    conn.execute_batch("CREATE TABLE graphs(iri VARCHAR PRIMARY KEY)")
        .unwrap();
    let mut rdf = RdfDatasetMapping::new();
    rdf.register_table(
        "graphs",
        schema_only_provider(Arc::new(Schema::new(vec![Field::new(
            "iri",
            DataType::Utf8,
            false,
        )]))),
    )
    .map_writable_named_graphs("default", "graphs", "iri");
    let mut engine =
        GraphEngine::mapped(conn, Arc::new(GraphMapping::new().with_rdf_mapping(rdf))).unwrap();
    engine
        .sparql_update("CREATE GRAPH <urn:empty>", "default", None)
        .await
        .unwrap();
    assert_eq!(
        engine
            .sparql_query("ASK {GRAPH <urn:empty> {}}", "default")
            .await
            .unwrap(),
        SparqlResults::Boolean(true)
    );
    assert_eq!(
        engine
            .sparql_query("ASK {?s ?p ?o}", "default")
            .await
            .unwrap(),
        SparqlResults::Boolean(false)
    );
    assert_eq!(
        engine
            .sparql_query("ASK {?s <urn:p> 'absent'}", "default")
            .await
            .unwrap(),
        SparqlResults::Boolean(false)
    );
}

#[tokio::test]
async fn native_literal_columns_have_valid_rdf_lexical_forms() {
    use arrow::datatypes::TimeUnit;
    let conn = duckdb::Connection::open_in_memory().unwrap();
    conn.execute_batch("CREATE TABLE events(id VARCHAR,occurred TIMESTAMP,value DOUBLE);INSERT INTO events VALUES('urn:event',TIMESTAMP '2020-01-02 03:04:05','inf'::DOUBLE)").unwrap();
    let mut map = GraphMapping::new();
    map.register_table(
        "events",
        schema_only_provider(Arc::new(Schema::new(vec![
            Field::new("id", DataType::Utf8, false),
            Field::new(
                "occurred",
                DataType::Timestamp(TimeUnit::Microsecond, None),
                false,
            ),
            Field::new("value", DataType::Float64, false),
        ]))),
    );
    for column in ["occurred", "value"] {
        map.map_rdf(RdfMapping::table(
            "events",
            T::iri("id"),
            format!("urn:{column}"),
            T::literal(column),
        ));
    }
    let mut engine = GraphEngine::mapped(conn, Arc::new(map)).unwrap();
    let SparqlResults::Solutions { rows, .. } = engine
        .sparql_query(
            "SELECT ?at ?value WHERE {?s <urn:occurred> ?at; <urn:value> ?value}",
            "default",
        )
        .await
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(
        rows[0][0],
        Some(RdfTermValue::typed(
            "2020-01-02T03:04:05",
            "http://www.w3.org/2001/XMLSchema#dateTime"
        ))
    );
    assert_eq!(
        rows[0][1],
        Some(RdfTermValue::typed(
            "INF",
            "http://www.w3.org/2001/XMLSchema#double"
        ))
    );
}
