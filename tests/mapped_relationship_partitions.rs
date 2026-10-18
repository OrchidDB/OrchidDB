#![cfg(feature = "duckdb")]
use arrow::datatypes::{DataType, Field, Schema};
use orchiddb::ir::{
    catalog::PropertyGraph,
    rel::{
        RelBackend, RelBackendOptions,
        mapping::{EdgeMapping, GraphMapping, NodeMapping},
        sql::{self, DuckDbExecutor, SqlDialect},
    },
};
use std::{collections::BTreeMap, sync::Arc};

async fn prepared(mapping: &GraphMapping, query: &str) -> sql::PreparedSql {
    let plan =
        orchiddb::language::cypher::preparation::prepare(query, &BTreeMap::new(), None).unwrap();
    let lowered = RelBackend::with_options(RelBackendOptions {
        mapping: Some(Arc::new(mapping.clone())),
        language_functions: true,
        ..Default::default()
    })
    .lower(&plan, &PropertyGraph::new())
    .unwrap();
    sql::prepare_with_external(
        &lowered,
        SqlDialect::DuckDb,
        &mapping.physical_table_names(),
    )
    .await
    .unwrap()
}
fn result(executor: &mut DuckDbExecutor, query: &sql::PreparedSql) -> Vec<String> {
    let result =
        sql::execute_prepared(executor, query).unwrap_or_else(|e| panic!("{e}\n{}", query.query));
    let mut rows = (0..result.batch.num_rows())
        .map(|row| {
            result
                .batch
                .columns()
                .iter()
                .map(|column| {
                    arrow::util::display::array_value_to_string(column.as_ref(), row).unwrap()
                })
                .collect::<Vec<_>>()
                .join("|")
        })
        .collect::<Vec<_>>();
    rows.sort();
    rows
}

fn fixture() -> (GraphMapping, DuckDbExecutor) {
    let connection = duckdb::Connection::open_in_memory().unwrap();
    connection.execute_batch("CREATE TABLE a(id BIGINT); CREATE TABLE b(id BIGINT); CREATE TABLE c(id BIGINT); INSERT INTO a VALUES (1); INSERT INTO b VALUES (1); INSERT INTO c VALUES (1); CREATE TABLE ab(id BIGINT, src BIGINT, dst BIGINT, p VARCHAR); CREATE TABLE ac(id BIGINT, src BIGINT, dst BIGINT, q VARCHAR); CREATE TABLE aa(id BIGINT, src BIGINT, dst BIGINT); INSERT INTO ab VALUES (10,1,1,'first'),(11,1,1,'parallel'); INSERT INTO ac VALUES (12,1,1,'second'); INSERT INTO aa VALUES (13,1,1)").unwrap();
    let mut mapping = GraphMapping::new();
    for (label, table) in [("A", "a"), ("B", "b"), ("C", "c")] {
        mapping.register_table_schema(
            table,
            Arc::new(Schema::new(vec![Field::new("id", DataType::Int64, false)])),
        );
        mapping.map_node(NodeMapping::table(label, table, "id"));
    }
    for (table, target, property) in [
        ("ab", "B", Some("p")),
        ("ac", "C", Some("q")),
        ("aa", "A", None),
    ] {
        let mut fields = vec![
            Field::new("id", DataType::Int64, false),
            Field::new("src", DataType::Int64, false),
            Field::new("dst", DataType::Int64, false),
        ];
        let mut edge = EdgeMapping::table("R", table, "src", "dst", "A", target).with_id("id");
        if let Some(property) = property {
            fields.push(Field::new(property, DataType::Utf8, true));
            edge = edge.property(property, property);
        }
        mapping.register_table_schema(table, Arc::new(Schema::new(fields)));
        mapping.map_edge_partition(edge);
    }
    (mapping, DuckDbExecutor::from_connection(connection))
}
#[tokio::test]
async fn partitioned_relationships_preserve_edges_labels_properties_and_direction() {
    let (mapping, mut executor) = fixture();
    for (query, expected) in [
        (
            "MATCH (a:A)-[r:R]->(b) RETURN type(r), labels(b), keys(r)",
            vec!["R|[A]|[]", "R|[B]|[p]", "R|[B]|[p]", "R|[C]|[q]"],
        ),
        ("MATCH (b)<-[r:R]-(a:A) RETURN count(r)", vec!["4"]),
        ("MATCH (a:A)-[r:R]-(b) RETURN count(r)", vec!["4"]),
        ("MATCH (a:A)-[r:R]->(b:B) RETURN count(r)", vec!["2"]),
        ("MATCH ()-[r:R]->() RETURN count(DISTINCT r)", vec!["4"]),
    ] {
        let sql = prepared(&mapping, query).await;
        assert_eq!(
            result(&mut executor, &sql),
            expected,
            "{query}: {}",
            sql.query
        );
    }
}
#[test]
fn replace_clears_partitions_and_serialization_never_drops_them() {
    let (mut mapping, _) = fixture();
    assert_eq!(mapping.edge_partitions("R").len(), 3);
    assert!(
        mapping
            .try_to_toml()
            .unwrap_err()
            .to_string()
            .contains("partitioned")
    );
    let facts = mapping.relationship_multiplicity("R").unwrap();
    assert!(!facts.at_most_one_outgoing && !facts.at_most_one_incoming);
    mapping.map_edge(EdgeMapping::table("R", "ab", "src", "dst", "A", "B").with_id("id"));
    assert_eq!(mapping.edge_partitions("R").len(), 1);
}

#[tokio::test]
async fn mapped_runtime_reads_all_relationship_partitions() {
    let (mapping, executor) = fixture();
    let mut engine = orchiddb::mapped_engine::MappedGraphEngine::new(executor, Arc::new(mapping));
    let result = engine
        .cypher("MATCH (a:A)-[r:R]->(b) RETURN count(r) AS n")
        .await
        .unwrap();
    assert_eq!(result.fields, vec!["n"]);
    assert_eq!(
        arrow::util::display::array_value_to_string(result.batch.column(0).as_ref(), 0).unwrap(),
        "4"
    );
}

#[tokio::test]
async fn compatible_partition_writes_route_updates_and_deletes_to_the_right_table() {
    let (mut mapping, mut executor) = fixture();
    executor
        .connection()
        .unwrap()
        .execute_batch("ALTER TABLE ac RENAME q TO p")
        .unwrap();
    mapping.register_table_schema(
        "ac",
        Arc::new(Schema::new(vec![
            Field::new("id", DataType::Int64, false),
            Field::new("src", DataType::Int64, false),
            Field::new("dst", DataType::Int64, false),
            Field::new("p", DataType::Utf8, true),
        ])),
    );
    mapping.map_edge(
        EdgeMapping::table("R", "ab", "src", "dst", "A", "B")
            .with_id("id")
            .property("value", "p"),
    );
    mapping.map_edge_partition(
        EdgeMapping::table("R", "ac", "src", "dst", "A", "C")
            .with_id("id")
            .property("value", "p"),
    );
    let mut engine = orchiddb::mapped_engine::MappedGraphEngine::new(executor, Arc::new(mapping));
    engine
        .cypher("MATCH ()-[r:R]->(:C) SET r.value = 'updated'")
        .await
        .unwrap();
    let db = engine.executor_mut().connection().unwrap();
    assert_eq!(
        db.query_row("SELECT p FROM ac", [], |row| row.get::<_, String>(0))
            .unwrap(),
        "updated"
    );
    assert_eq!(
        db.query_row("SELECT p FROM ab WHERE id=10", [], |row| row
            .get::<_, String>(0))
            .unwrap(),
        "first"
    );
    engine
        .cypher("MATCH ()-[r:R]->(:C) DELETE r")
        .await
        .unwrap();
    let db = engine.executor_mut().connection().unwrap();
    assert_eq!(
        db.query_row("SELECT count(*) FROM ab", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        2
    );
    assert_eq!(
        db.query_row("SELECT count(*) FROM ac", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        0
    );
}

#[tokio::test]
async fn partition_insert_uses_endpoint_route_and_global_identity_uniqueness() {
    let (mut mapping, executor) = fixture();
    mapping.map_edge(
        EdgeMapping::table("R", "ab", "src", "dst", "A", "B")
            .with_id("id")
            .property("id", "id"),
    );
    mapping.map_edge_partition(
        EdgeMapping::table("R", "ac", "src", "dst", "A", "C")
            .with_id("id")
            .property("id", "id"),
    );
    let mut engine = orchiddb::mapped_engine::MappedGraphEngine::new(executor, Arc::new(mapping));
    engine
        .cypher("MATCH (a:A), (c:C) CREATE (a)-[:R {id: 20}]->(c)")
        .await
        .unwrap();
    assert_eq!(
        engine
            .executor_mut()
            .connection()
            .unwrap()
            .query_row("SELECT count(*) FROM ac WHERE id=20", [], |row| row
                .get::<_, i64>(0))
            .unwrap(),
        1
    );
    let error = engine
        .cypher("MATCH (a:A), (b:B) CREATE (a)-[:R {id: 20}]->(b)")
        .await
        .unwrap_err();
    assert!(error.contains("duplicate primary key"), "{error}");
    assert_eq!(
        engine
            .executor_mut()
            .connection()
            .unwrap()
            .query_row("SELECT count(*) FROM ab WHERE id=20", [], |row| row
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
}
