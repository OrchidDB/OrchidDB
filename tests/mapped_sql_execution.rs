#![cfg(feature = "duckdb")]
use orchiddb::{
    engine::GraphEngine,
    ir::rel::mapping::{EdgeMapping, GraphMapping, NodeMapping},
};
use std::sync::Arc;
fn engine() -> GraphEngine {
    let db = duckdb::Connection::open_in_memory().unwrap();
    db.execute_batch("CREATE TABLE people(id VARCHAR PRIMARY KEY, name VARCHAR, unused VARCHAR); INSERT INTO people SELECT 'p'||i, 'name'||i, repeat('x',100) FROM range(10000) t(i); CREATE TABLE links(id VARCHAR PRIMARY KEY,src VARCHAR,dst VARCHAR); INSERT INTO links SELECT 'e'||i,'p'||i,'p'||(i+1) FROM range(9999) t(i); CREATE VIEW unrelated AS SELECT error('unrelated mapping was scanned')::VARCHAR AS id FROM range(1);").unwrap();
    let mut m = GraphMapping::new();
    m.map_node(
        NodeMapping::table("Person", "people", "id")
            .property("id", "id")
            .property("name", "name"),
    );
    m.map_node(NodeMapping::table("Unrelated", "unrelated", "id"));
    m.map_edge(EdgeMapping::table("LINK", "links", "src", "dst", "Person", "Person").with_id("id"));
    GraphEngine::mapped(db, Arc::new(m)).unwrap()
}

#[cfg(feature = "postgres")]
#[tokio::test]
async fn mapped_sources_execute_in_caller_owned_postgres_without_ddl() {
    use orchiddb::ir::rel::sql::{self, region::{RegionSession, PostgresRegionSession}};
    let Ok(url) = std::env::var("GRAPH_PG_URL") else { return; };
    let client = std::thread::spawn(move || postgres::Client::connect(&url, postgres::NoTls).unwrap()).join().unwrap();
    #[derive(Debug)]
    struct ReadOnly(PostgresRegionSession);
    impl RegionSession for ReadOnly {
        fn dialect(&self) -> sql::SqlDialect { sql::SqlDialect::Postgres }
        fn query(&mut self, query: &str, schema: arrow::datatypes::SchemaRef) -> sql::SqlResult<arrow::record_batch::RecordBatch> {
            use datafusion::sql::sqlparser::{parser::Parser, dialect::PostgreSqlDialect, ast::Statement};
            let statements = Parser::parse_sql(&PostgreSqlDialect {}, query).unwrap();
            assert_eq!(statements.len(), 1);
            assert!(matches!(statements[0], Statement::Query(_)));
            self.0.query(query, schema)
        }
    }
    let mut engine = engine();
    engine.set_sql_region_session(Box::new(ReadOnly(PostgresRegionSession::new(client))));
    let result = engine.cypher("MATCH (p:Person {id:'p42'}) RETURN p.name AS name").await.unwrap();
    assert_eq!(result.returned.batch.num_rows(), 1);
    assert_eq!(arrow::util::display::array_value_to_string(result.returned.batch.column(0), 0).unwrap(), "name42");
    assert_eq!(result.stats.duckdb_regions, 0);
    assert!(result.stats.postgres_regions > 0);
}
#[tokio::test]
async fn selective_read_executes_filtered_sql_without_native_source_scan() {
    let mut e = engine();
    let r = e
        .cypher("MATCH (p:Person {id:'p42'}) RETURN p.name")
        .await
        .unwrap();
    assert_eq!(r.returned.batch.num_rows(), 1);
    assert!(r.stats.islands > 0, "{:?}", r.stats);
    assert_eq!(r.stats.native_source_rows, 0, "{:?}", r.stats);
    assert!(
        r.stats
            .sql_queries
            .iter()
            .any(|s| s.contains("p42") && s.contains("WHERE")),
        "{:?}",
        r.stats
    );
    assert!(
        r.stats
            .sql_queries
            .iter()
            .all(|s| !s.contains("unrelated") && !s.contains("links") && !s.contains("unused")),
        "{:?}",
        r.stats
    );
}
#[tokio::test]
async fn selective_write_reads_only_affected_records() {
    let mut e = engine();
    let r = e
        .cypher("MATCH (p:Person {id:'p42'}) SET p.name='changed' RETURN p.name")
        .await
        .unwrap();
    assert_eq!(r.returned.batch.num_rows(), 1);
    assert!(r.stats.islands > 0, "{:?}", r.stats);
    assert!(r.stats.native_source_rows <= 2, "{:?}", r.stats);
    let r = e
        .cypher("MATCH (p:Person {name:'changed'}) RETURN count(p)")
        .await
        .unwrap();
    assert_eq!(
        arrow::util::display::array_value_to_string(r.returned.batch.column(0), 0).unwrap(),
        "1"
    );
}
#[tokio::test]
async fn native_repeat_fetches_only_reachable_frontier() {
    let mut e = engine();
    let r = e
        .gremlin(
            "g.V().hasLabel('Person').has('id','p42').repeat(out('LINK')).times(3).values('name')",
        )
        .await
        .unwrap();
    assert_eq!(r.returned.batch.num_rows(), 1);
    assert!(r.stats.islands > 0, "{:?}", r.stats);
    assert!(r.stats.native_source_rows < 20, "{:?}", r.stats);
    assert!(
        r.stats
            .native_source_queries
            .iter()
            .all(|s| s.contains("WHERE") && !s.contains("unrelated")),
        "{:?}",
        r.stats
    );
}
#[tokio::test]
async fn native_frontier_lookups_are_batched() {
    let mut e = engine();
    let r=e.gremlin("g.V().hasLabel('Person').has('id',within('p42','p142','p242','p342')).repeat(out('LINK')).times(2).values('name')").await.unwrap();
    assert_eq!(r.returned.batch.num_rows(), 4);
    assert!(r.stats.native_source_queries.len() <= 8, "{:?}", r.stats);
    assert!(r.stats.native_source_rows < 40, "{:?}", r.stats);
}
#[tokio::test]
async fn mixed_width_keys_keep_their_types_across_native_boundaries() {
    let db = duckdb::Connection::open_in_memory().unwrap();
    db.execute_batch("CREATE TABLE a(id BIGINT PRIMARY KEY,name VARCHAR); INSERT INTO a VALUES (-1,'signed'); CREATE TABLE b(id UBIGINT PRIMARY KEY,name VARCHAR); INSERT INTO b VALUES (18446744073709551615,'unsigned');").unwrap();
    let mut m = GraphMapping::new();
    m.map_node(NodeMapping::table("A", "a", "id").property("name", "name"));
    m.map_node(NodeMapping::table("B", "b", "id").property("name", "name"));
    let mut e = GraphEngine::mapped(db, Arc::new(m)).unwrap();
    let r = e.gremlin("g.V().valueMap('name')").await.unwrap();
    assert_eq!(r.returned.batch.num_rows(), 2);
    let output = format!("{:?}", r.returned.batch);
    assert!(
        output.contains("signed") && output.contains("unsigned"),
        "{output}"
    );
    assert!(r.stats.islands > 0, "{:?}", r.stats);
}
#[tokio::test]
async fn dropping_a_query_restores_the_original_connection() {
    use std::{future::Future, task::Poll};
    let mut e = engine();
    let mut query = Box::pin(e.cypher("MATCH (p:Person) RETURN count(p)"));
    futures::future::poll_fn(|cx| {
        let _ = query.as_mut().poll(cx);
        Poll::Ready(())
    })
    .await;
    drop(query);
    e.begin().unwrap();
    let r = e
        .cypher("MATCH (p:Person {id:'p42'}) RETURN p.name")
        .await
        .unwrap();
    assert_eq!(r.returned.batch.num_rows(), 1);
    e.rollback().unwrap();
}

#[tokio::test]
async fn shortest_path_reads_only_the_reachable_frontier() {
    let mut e = engine();
    use orchiddb::ir::{
        ElementId, Value,
        plan::{Direction, GraphPlan, LabelExpr, Node},
        policy::{GraphPlanPolicy, ResultForm},
    };
    let node = |key: &str| Value::Node {
        label: "Person".into(),
        id: ElementId::try_from(&Value::String(key.into())).unwrap(),
    };
    let plan = GraphPlan::new(
        GraphPlanPolicy::cypher(),
        Node::GraphReturn {
            fields: vec!["path".into()],
            result_form: ResultForm::RowSet,
            input: Box::new(Node::GraphShortestPath {
                source: "a".into(),
                target: Some("b".into()),
                direction: Direction::Out,
                rel_types: LabelExpr::AnyOf(vec!["LINK".into()]),
                max_distance: Some(5.0),
                include_edges: true,
                output: "path".into(),
                all_paths: false,
                input: Box::new(Node::GraphValues {
                    bindings: vec!["a".into(), "b".into()],
                    rows: vec![vec![node("p42"), node("p45")]],
                    bulk: None,
                }),
            }),
        },
    );
    let r = e.execute_plan(&plan).await.unwrap();
    assert_eq!(r.returned.batch.num_rows(), 1);
    let path = arrow::util::display::array_value_to_string(r.returned.batch.column(0), 0).unwrap();
    assert!(path.contains("p42") && path.contains("p45"), "{path}");
    assert!(r.stats.native_source_rows < 20, "{:?}", r.stats);
    assert!(
        r.stats
            .native_source_queries
            .iter()
            .all(|sql| !sql.contains("unrelated")),
        "{:?}",
        r.stats
    );
}

#[tokio::test]
async fn named_path_only_reads_possible_endpoint_mappings() {
    let mut e = engine();
    let result = e.cypher("MATCH p=(a:Person {id:'p42'})-[:LINK*1..3]->(b:Person) RETURN length(p) AS hops,b.name ORDER BY hops").await.unwrap();
    assert_eq!(result.returned.batch.num_rows(), 3);
    assert!(result.stats.islands > 0);
    assert!(result.stats.sql_queries.iter().chain(&result.stats.native_source_queries).all(|sql| !sql.contains("unrelated")));
    for row in 0..3 {
        assert_eq!(arrow::util::display::array_value_to_string(result.returned.batch.column(0), row).unwrap(), (row+1).to_string());
        assert_eq!(arrow::util::display::array_value_to_string(result.returned.batch.column(1), row).unwrap(), format!("name{}",row+43));
    }
}
