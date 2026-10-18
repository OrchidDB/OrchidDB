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

fn fixture() -> (GraphMapping, DuckDbExecutor) {
    let connection = duckdb::Connection::open_in_memory().unwrap();
    connection.execute_batch("CREATE TABLE nodes(id BIGINT PRIMARY KEY, labels VARCHAR[] NOT NULL); INSERT INTO nodes VALUES (1,['A','B']),(2,['A']),(3,['B']),(4,[]); CREATE TABLE edges(id BIGINT, src BIGINT, dst BIGINT); INSERT INTO edges VALUES (10,1,2),(11,1,2),(12,1,1),(13,3,1)").unwrap();
    let mut mapping = GraphMapping::new();
    mapping.register_table_schema(
        "nodes",
        Arc::new(Schema::new(vec![
            Field::new("id", DataType::Int64, false),
            Field::new(
                "labels",
                DataType::List(Arc::new(Field::new("item", DataType::Utf8, true))),
                false,
            ),
        ])),
    );
    mapping.register_table_schema(
        "edges",
        Arc::new(Schema::new(
            ["id", "src", "dst"]
                .map(|name| Field::new(name, DataType::Int64, false))
                .to_vec(),
        )),
    );
    mapping.map_node(
        NodeMapping::table("physical", "nodes", "id")
            .labels_column("labels")
            .property("id", "id"),
    );
    mapping.map_edge(
        EdgeMapping::table("R", "edges", "src", "dst", "physical", "physical").with_id("id"),
    );
    (mapping, DuckDbExecutor::from_connection(connection))
}
async fn rows(mapping: &GraphMapping, executor: &mut DuckDbExecutor, query: &str) -> Vec<String> {
    let plan =
        orchiddb::language::cypher::preparation::prepare(query, &BTreeMap::new(), None).unwrap();
    let lowered = RelBackend::with_options(RelBackendOptions {
        mapping: Some(Arc::new(mapping.clone())),
        language_functions: true,
        ..Default::default()
    })
    .lower(&plan, &PropertyGraph::new())
    .unwrap();
    let prepared = sql::prepare_with_external(
        &lowered,
        SqlDialect::DuckDb,
        &mapping.physical_table_names(),
    )
    .await
    .unwrap();
    let result = sql::execute_prepared(executor, &prepared)
        .unwrap_or_else(|e| panic!("{e}\n{}", prepared.query));
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
#[tokio::test]
async fn label_membership_keeps_one_physical_identity_and_correct_edges() {
    let (mapping, mut executor) = fixture();
    for (query, expected) in [
        ("MATCH (n) RETURN count(n)", vec!["4"]),
        ("MATCH (n:A) RETURN n.id", vec!["1", "2"]),
        ("MATCH (n:A:B) RETURN n.id, labels(n)", vec!["1|[A, B]"]),
        ("MATCH (n) WHERE n:B RETURN n.id", vec!["1", "3"]),
        ("MATCH (n) WHERE NOT n:A RETURN n.id", vec!["3", "4"]),
        (
            "MATCH (a:A:B)-[r:R]->(b:A) RETURN a.id,b.id",
            vec!["1|1", "1|2", "1|2"],
        ),
        (
            "MATCH (a:B)<-[r:R]-(b:B) RETURN a.id,b.id",
            vec!["1|1", "1|3"],
        ),
        (
            "MATCH (n:A:B) OPTIONAL MATCH (n)-[:R]->(m:Missing) RETURN n.id,m.id",
            vec!["1|"],
        ),
    ] {
        assert_eq!(
            rows(&mapping, &mut executor, query).await,
            expected,
            "{query}"
        );
    }
    let serialized = mapping.to_toml();
    let restored = GraphMapping::from_toml(&serialized).unwrap();
    assert_eq!(
        restored.node("physical").unwrap().labels_column.as_deref(),
        Some("labels")
    );
}

#[tokio::test]
async fn label_updates_persist_and_native_fallback_reads_memberships() {
    let (mapping, executor) = fixture();
    let mut engine = orchiddb::mapped_engine::MappedGraphEngine::new(executor, Arc::new(mapping));
    let returned = engine
        .cypher("MATCH (n:A:B) WITH n AS alias RETURN labels(alias), alias.id")
        .await
        .unwrap();
    let labels =
        arrow::util::display::array_value_to_string(returned.batch.column(0).as_ref(), 0).unwrap();
    assert!(labels.contains("A") && labels.contains("B"), "{labels}");
    engine
        .cypher("MATCH (n:A:B) SET n:C REMOVE n:A RETURN labels(n)")
        .await
        .unwrap();
    let labels: String = engine
        .executor_mut()
        .connection()
        .unwrap()
        .query_row(
            "SELECT CAST(labels AS VARCHAR) FROM nodes WHERE id=1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(labels, "[B, C]");
    engine
        .cypher("MATCH (n:C) SET n:D RETURN labels(n)")
        .await
        .unwrap();
    let labels: String = engine
        .executor_mut()
        .connection()
        .unwrap()
        .query_row(
            "SELECT CAST(labels AS VARCHAR) FROM nodes WHERE id=1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(labels, "[B, C, D]");
}

#[tokio::test]
async fn whole_multi_label_node_display_is_not_lowered_to_lossy_physical_label_text() {
    let (mapping, _) = fixture();
    let plan = orchiddb::language::cypher::preparation::prepare(
        "MATCH (n:A:B) RETURN n",
        &BTreeMap::new(),
        None,
    )
    .unwrap();
    let result = RelBackend::with_options(RelBackendOptions {
        mapping: Some(Arc::new(mapping)),
        language_functions: true,
        ..Default::default()
    })
    .lower(&plan, &PropertyGraph::new());
    assert!(result.is_err());
}
