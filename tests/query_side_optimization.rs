#![cfg(feature = "duckdb")]
use orchiddb::{
    engine::{GraphEngine, QueryResult},
    ir::rel::mapping::{EdgeMapping, GraphMapping, NodeMapping},
};
use std::sync::Arc;

fn rows(result: &QueryResult) -> Vec<Vec<String>> {
    let batch = &result.returned.batch;
    let mut rows = (0..batch.num_rows())
        .map(|row| {
            batch
                .columns()
                .iter()
                .map(|column| {
                    arrow::util::display::array_value_to_string(column.as_ref(), row).unwrap()
                })
                .collect()
        })
        .collect::<Vec<Vec<String>>>();
    rows.sort();
    rows
}
fn fixture() -> GraphEngine {
    let db = duckdb::Connection::open_in_memory().unwrap();
    db.execute_batch("CREATE TABLE sources(id BIGINT PRIMARY KEY, labels VARCHAR[], n BIGINT); INSERT INTO sources VALUES (1,['S'],10),(2,['S'],20); CREATE TABLE targets(id BIGINT PRIMARY KEY, labels VARCHAR[], n BIGINT); INSERT INTO targets VALUES (3,['T'],30),(4,['T'],NULL); CREATE TABLE links(id BIGINT PRIMARY KEY,src BIGINT,dst BIGINT); INSERT INTO links VALUES (1,1,3),(2,1,3),(3,1,4); CREATE VIEW unrelated AS SELECT error('unrelated endpoint scanned')::BIGINT AS id, ['U'] AS labels").unwrap();
    let mut m = GraphMapping::new();
    m.map_node(
        NodeMapping::table("S", "sources", "id")
            .labels_column("labels")
            .property("n", "n"),
    );
    m.map_node(
        NodeMapping::table("T", "targets", "id")
            .labels_column("labels")
            .property("n", "n"),
    );
    m.map_node(NodeMapping::table("U", "unrelated", "id").labels_column("labels"));
    m.map_edge(EdgeMapping::table("R", "links", "src", "dst", "S", "T").with_id("id"));
    GraphEngine::mapped(db, Arc::new(m)).unwrap()
}

#[tokio::test]
async fn endpoint_joins_require_proven_integrity() {
    use orchiddb::ir::rel::constraints::{Constraint, ConstraintCatalog, Fact};
    for proven in [false, true] {
        let db = duckdb::Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE nodes(id BIGINT PRIMARY KEY); INSERT INTO nodes VALUES(1),(2); CREATE TABLE links(id BIGINT PRIMARY KEY,src BIGINT NOT NULL REFERENCES nodes(id),dst BIGINT NOT NULL REFERENCES nodes(id),weight BIGINT); INSERT INTO links VALUES(1,1,2,42)").unwrap();
        let mut m = GraphMapping::new();
        m.map_node(NodeMapping::table("N", "nodes", "id"));
        m.map_edge(
            EdgeMapping::table("R", "links", "src", "dst", "N", "N")
                .with_id("id")
                .property("weight", "weight"),
        );
        if proven {
            let mut facts = ConstraintCatalog::default();
            facts.insert(
                "nodes",
                Constraint::enforced(
                    "pk",
                    Fact::Unique {
                        columns: vec!["id".into()],
                        nulls_equal: false,
                    },
                ),
            );
            facts.insert(
                "nodes",
                Constraint::enforced(
                    "nn",
                    Fact::NonNull {
                        columns: vec!["id".into()],
                    },
                ),
            );
            facts.insert(
                "links",
                Constraint::enforced(
                    "nn",
                    Fact::NonNull {
                        columns: vec!["src".into(), "dst".into()],
                    },
                ),
            );
            for endpoint in ["src", "dst"] {
                facts.insert(
                    "links",
                    Constraint::enforced(
                        endpoint,
                        Fact::ForeignKey {
                            columns: vec![endpoint.into()],
                            target: "nodes".into(),
                            references: vec!["id".into()],
                        },
                    ),
                );
            }
            m.set_constraints(facts);
        }
        let mut engine = GraphEngine::mapped(db, Arc::new(m)).unwrap();
        let result = engine
            .cypher("MATCH ()-[r:R]->() RETURN r.weight")
            .await
            .unwrap();
        assert_eq!(rows(&result), vec![vec!["42".to_string()]]);
        let sql = result.stats.sql_queries.join("\n").to_lowercase();
        assert_eq!(!sql.contains("join"), proven, "{sql}");
    }
}

#[tokio::test]
async fn collection_consumers_keep_null_empty_and_duplicate_elements() {
    let mut engine = GraphEngine::in_memory().unwrap();
    let result = engine.cypher("UNWIND [[1,null,1],[],null] AS xs RETURN size([x IN xs WHERE x=1 | x]),size([x IN xs | 7]),head([x IN xs | 7])").await.unwrap();
    let mut expected: Vec<Vec<String>> = vec![
        vec!["2".into(), "3".into(), "7".into()],
        vec!["0".into(), "0".into(), "".into()],
        vec!["".into(), "".into(), "".into()],
    ];
    expected.sort();
    assert_eq!(rows(&result), expected);
    let sql = result.stats.sql_queries.join("\n").to_lowercase();
    assert!(sql.contains("list_sum"), "{sql}");
    assert!(
        !sql.contains("array_agg") && !sql.contains("row_number"),
        "{sql}"
    );
    assert_eq!(result.stats.cost.sql_executions, 1);
}

#[tokio::test]
async fn typed_conversion_uses_native_sql() {
    let db = duckdb::Connection::open_in_memory().unwrap();
    db.execute_batch("CREATE TABLE vals(id BIGINT PRIMARY KEY, n INTEGER, b BOOLEAN); INSERT INTO vals VALUES (1,42,true),(2,NULL,NULL)").unwrap();
    let mut m = GraphMapping::new();
    m.map_node(
        NodeMapping::table("V", "vals", "id")
            .property("n", "n")
            .property("b", "b"),
    );
    let mut engine = GraphEngine::mapped(db, Arc::new(m)).unwrap();
    let result = engine
        .cypher("MATCH(n:V) RETURN toInteger(n.n),toBoolean(n.b)")
        .await
        .unwrap();
    assert_eq!(
        rows(&result),
        vec![
            vec!["".to_string(), "".into()],
            vec!["42".into(), "true".into()]
        ]
    );
    let sql = result.stats.sql_queries.join("\n");
    assert!(!sql.contains("orchid_cypher_convert"), "{sql}");
}

#[tokio::test]
async fn missing_property_predicate_removes_impossible_union_branch() {
    let db = duckdb::Connection::open_in_memory().unwrap();
    db.execute_batch("CREATE TABLE a(id BIGINT PRIMARY KEY, score BIGINT); INSERT INTO a VALUES(1,42); CREATE TABLE b(id BIGINT PRIMARY KEY, other VARCHAR); INSERT INTO b VALUES(2,'unused')").unwrap();
    let mut m = GraphMapping::new();
    m.map_node(NodeMapping::table("A", "a", "id").property("score", "score"));
    m.map_node(NodeMapping::table("B", "b", "id").property("other", "other"));
    let mut engine = GraphEngine::mapped(db, Arc::new(m)).unwrap();
    let result = engine
        .cypher("MATCH(n) WHERE n.score IS NOT NULL RETURN n.score")
        .await
        .unwrap();
    assert_eq!(rows(&result), vec![vec!["42".to_string()]]);
    let sql = result.stats.sql_queries.join("\n");
    assert!(
        !sql.contains("FROM \"b\"") && !sql.contains("UNION"),
        "{sql}"
    );
}

#[tokio::test]
async fn physical_endpoint_pruning_keeps_dynamic_label_checks() {
    let mut engine = fixture();
    // The unlabelled anchor is intentionally supplied by an edge endpoint:
    // its physical identity excludes U even though logical labels are dynamic.
    let result = engine
        .cypher("MATCH ()-[r:R]->(b) RETURN b.n")
        .await
        .unwrap();
    assert_eq!(
        rows(&result),
        vec![vec!["".to_string()], vec!["30".into()], vec!["30".into()]]
    );
    let sql = result.stats.sql_queries.join("\n");
    assert!(!sql.contains("unrelated"), "{sql}");
}

#[tokio::test]
async fn grouped_pattern_collection_preserves_outer_duplicates_and_null_items() {
    // No error view: the source MATCH is allowed to scan every dynamic-label table.
    let db = duckdb::Connection::open_in_memory().unwrap();
    db.execute_batch("CREATE TABLE nodes(id BIGINT PRIMARY KEY,n BIGINT); INSERT INTO nodes VALUES (1,10),(2,20),(3,NULL); CREATE TABLE links(id BIGINT PRIMARY KEY,src BIGINT,dst BIGINT); INSERT INTO links VALUES (1,1,2),(2,1,2),(3,1,3)").unwrap();
    let mut m = GraphMapping::new();
    m.map_node(NodeMapping::table("N", "nodes", "id").property("n", "n"));
    m.map_edge(EdgeMapping::table("R", "links", "src", "dst", "N", "N").with_id("id"));
    let mut engine = GraphEngine::mapped(db, Arc::new(m)).unwrap();
    let result = engine
        .cypher("UNWIND [1,1] AS duplicate MATCH(n:N) RETURN n.n,[(n)-[:R]->(b) | b.n] AS xs")
        .await
        .unwrap();
    assert_eq!(result.returned.batch.num_rows(), 6);
    let sql = result.stats.sql_queries.join("\n");
    assert!(sql.contains("pattern_group"), "{sql}");
    assert!(!sql.to_lowercase().contains("row_number"), "{sql}");
    assert_eq!(result.stats.cost.sql_executions, 1);
    let values: serde_json::Value = serde_json::from_str(
        &result.returned.batch.schema().metadata()["orchiddb.cypher.typed_rows.v1"],
    )
    .unwrap();
    let lists = values
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r[1]["value"].as_array().unwrap().len())
        .collect::<Vec<_>>();
    assert_eq!(lists.iter().filter(|&&n| n == 3).count(), 2);
    assert_eq!(lists.iter().filter(|&&n| n == 0).count(), 4);
    let volatile = engine
        .cypher("MATCH(n:N) WITH n,rand() AS token RETURN token,[(n)-[:R]->(b) | b.n]")
        .await
        .unwrap();
    assert_eq!(volatile.returned.batch.num_rows(), 3);
    assert!(
        !volatile
            .stats
            .sql_queries
            .join("\n")
            .contains("pattern_group"),
        "volatile outer inputs must keep their occurrence scope"
    );
}
