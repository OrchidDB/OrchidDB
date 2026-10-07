use duckdb::arrow::{
    array::{Array, StringArray},
    datatypes::SchemaRef,
    error::ArrowError,
    record_batch::{RecordBatch, RecordBatchReader},
};
use duckdb::{Connection, Statement};
struct Batches<'a>(duckdb::Arrow<'a>);
impl Iterator for Batches<'_> {
    type Item = Result<RecordBatch, ArrowError>;
    fn next(&mut self) -> Option<Self::Item> {
        self.0.next().map(Ok)
    }
}
impl RecordBatchReader for Batches<'_> {
    fn schema(&self) -> SchemaRef {
        self.0.get_schema()
    }
}
use orchiddb_client::{
    ExecutionError, SqlDialect, SqlSession,
    compiler::{CompiledSql, compile},
    execute,
};
use serde_json::{Value, json};

// A test application adapter, not an OrchidDB-owned connection or runtime.
struct Session<'db> {
    db: &'db Connection,
    statement: Option<Statement<'db>>,
    calls: usize,
}
impl<'db> Session<'db> {
    fn new(db: &'db Connection) -> Self {
        Self {
            db,
            statement: None,
            calls: 0,
        }
    }
}
impl SqlSession for Session<'_> {
    type Error = duckdb::Error;
    type Output<'a>
        = Batches<'a>
    where
        Self: 'a;
    fn dialect(&self) -> SqlDialect {
        SqlDialect::DuckDb
    }
    async fn query<'a>(&'a mut self, query: &CompiledSql) -> Result<Batches<'a>, Self::Error> {
        self.calls += 1;
        self.statement = Some(self.db.prepare(&query.sql)?);
        self.statement
            .as_mut()
            .unwrap()
            .query_arrow([])
            .map(Batches)
    }
}
const ADVERSARIAL_NAME: &str = "O'Reilly; DROP TABLE people; --";
fn database() -> Connection {
    let db = Connection::open_in_memory().unwrap();
    db.execute_batch(
        "CREATE TABLE people(id BIGINT PRIMARY KEY, name VARCHAR);
        CREATE TABLE follows(id BIGINT PRIMARY KEY, src BIGINT, dst BIGINT);
        INSERT INTO people VALUES (1, 'Ada'), (2, 'Grace');
        INSERT INTO follows VALUES (10, 1, 2), (11, 2, 3);
        CREATE MACRO application_upper(s) AS upper(s);",
    )
    .unwrap();
    db.execute("INSERT INTO people VALUES (3, ?)", [ADVERSARIAL_NAME])
        .unwrap();
    db
}
fn request(language: &str, query: &str) -> Value {
    json!({
        "version": 1, "dialect": "duckdb", "language": language, "query": query,
        "tables": [
            {"name": "people", "columns": [
                {"name": "id", "data_type": "int64", "nullable": false},
                {"name": "name", "data_type": "string"}]},
            {"name": "follows", "columns": [
                {"name": "id", "data_type": "int64", "nullable": false},
                {"name": "src", "data_type": "int64"},
                {"name": "dst", "data_type": "int64"}]}
        ],
        "nodes": [{"label": "Person", "table": "people", "id": "id", "properties": {"name": "name"}}],
        "edges": [{"label": "FOLLOWS", "table": "follows", "id": "id", "source": "src", "target": "dst",
                   "source_label": "Person", "target_label": "Person"}],
        "ontology": {
            "classes": [{"iri": "http://example.org/Person", "label": "Person"}],
            "properties": [{"iri": "http://example.org/name", "label": "Person", "property": "name"}]
        }
    })
}
async fn plan(request: Value) -> CompiledSql {
    compile(serde_json::from_value(request).unwrap())
        .await
        .unwrap()
}
async fn names(session: &mut Session<'_>, compiled: &CompiledSql) -> Vec<String> {
    let mut rows = execute(session, compiled).await.unwrap();
    let mut result = Vec::new();
    for batch in &mut rows {
        let batch = batch.unwrap();
        let names = batch
            .column(0)
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        result.extend(names.iter().map(|v| v.unwrap().to_owned()));
    }
    result
}
fn count(db: &Connection) -> i64 {
    db.query_row("SELECT count(*) FROM people", [], |r| r.get(0))
        .unwrap()
}

#[tokio::test]
async fn cypher_maps_nodes_and_relationships_against_real_tables() {
    let db = database();
    let query = plan(request("cypher", "MATCH (a:Person)-[:FOLLOWS]->(b:Person) RETURN a.name AS source, b.name AS target ORDER BY source")).await;
    assert_eq!(query.fields, ["source", "target"]);
    let mut session = Session::new(&db);
    let mut rows = execute(&mut session, &query).await.unwrap();
    let mut pairs = Vec::new();
    for batch in &mut rows {
        let batch = batch.unwrap();
        let a = batch
            .column(0)
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        let b = batch
            .column(1)
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        pairs.extend((0..batch.num_rows()).map(|i| (a.value(i).to_owned(), b.value(i).to_owned())));
    }
    assert_eq!(
        pairs,
        [
            ("Ada".into(), "Grace".into()),
            ("Grace".into(), ADVERSARIAL_NAME.into())
        ]
    );
}

#[tokio::test]
async fn permission_scopes_filter_direct_and_project_grants() {
    let db = Connection::open_in_memory().unwrap();
    db.execute_batch(
        "CREATE TABLE documents(id BIGINT, project_id BIGINT, title VARCHAR);
        INSERT INTO documents VALUES (1, 10, 'direct'), (2, 20, 'project'), (3, 30, 'denied');
        CREATE TABLE grants(resource_type VARCHAR, resource_rel VARCHAR, resource_id VARCHAR,
            subject_type VARCHAR, subject_rel VARCHAR, subject_id VARCHAR);
        INSERT INTO grants VALUES ('document','view','1','user','','alice'),
            ('project','view','20','user','','alice'), ('project','view','30','user','','bob');",
    )
    .unwrap();
    let request = json!({
        "version":1, "dialect":"duckdb", "language":"cypher",
        "query":"MATCH (d:Document) RETURN d.title AS title ORDER BY title",
        "authorization":{"subject_type":"user", "subject_id":"alice"},
        "tables":[
            {"name":"documents", "columns":[
                {"name":"id", "data_type":"int64"}, {"name":"project_id", "data_type":"int64"},
                {"name":"title", "data_type":"string"}]},
            {"name":"grants", "columns":[
                {"name":"resource_type", "data_type":"string"}, {"name":"resource_rel", "data_type":"string"},
                {"name":"resource_id", "data_type":"string"}, {"name":"subject_type", "data_type":"string"},
                {"name":"subject_rel", "data_type":"string"}, {"name":"subject_id", "data_type":"string"}]}
        ],
        "nodes":[{"label":"Document", "table":"documents", "id":"id",
            "properties":{"title":"title", "project_id":"project_id"},
            "permission_scopes":[
                {"resource_column":"id", "relation":{"table":"grants", "resource_type":"document", "permission":"view"}},
                {"resource_column":"project_id", "relation":{"table":"grants", "resource_type":"project", "permission":"view"}}
            ]}]
    });
    let compiled = plan(request.clone()).await;
    let mut statement = db.prepare(&compiled.sql).unwrap();
    let mut batches = statement.query_arrow([]).unwrap();
    let mut names = Vec::new();
    while let Some(batch) = batches.next() {
        let values = batch
            .column(0)
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        names.extend(values.iter().map(|value| value.unwrap().to_string()));
    }
    assert_eq!(names, ["direct", "project"]);

    let no_principal = serde_json::from_value::<orchiddb::compiler::CompileRequest>({
        let mut request = request.clone();
        request.as_object_mut().unwrap().remove("authorization");
        request
    })
    .unwrap();
    let error = compile(no_principal).await.unwrap_err();
    assert!(error.contains("requires a principal"));
}

#[tokio::test]
async fn gremlin_traverses_mapped_relationships() {
    let db = database();
    let query = plan(request("gremlin", "g.V(1).out('FOLLOWS').values('name')")).await;
    assert_eq!(names(&mut Session::new(&db), &query).await, ["Grace"]);
}

#[tokio::test]
async fn sparql_ontology_returns_actual_expected_rows() {
    let db = database();
    let query = plan(request("sparql", "SELECT ?name WHERE { ?p a <http://example.org/Person> ; <http://example.org/name> ?name . } ORDER BY ?name")).await;
    assert_eq!(
        names(&mut Session::new(&db), &query).await,
        ["Ada", "Grace", ADVERSARIAL_NAME]
    );
}

#[tokio::test]
async fn parameters_are_data_and_specialized_plans_remain_distinct() {
    let db = database();
    let mut session = Session::new(&db);
    for value in [ADVERSARIAL_NAME, "Ada", "absent"] {
        let mut r = request(
            "cypher",
            "MATCH (p:Person) WHERE p.name=$name RETURN p.name AS name",
        );
        r["parameters"] = json!({"name": value});
        let query = plan(r).await;
        let expected = if value == "absent" {
            vec![]
        } else {
            vec![value.to_owned()]
        };
        assert_eq!(names(&mut session, &query).await, expected);
    }
    assert_eq!(count(&db), 3);
}

#[tokio::test]
async fn application_function_runs_on_the_callers_connection() {
    let db = database();
    let mut r = request(
        "cypher",
        "MATCH (p:Person) WHERE p.name='Ada' RETURN decorate(p.name) AS name",
    );
    r["functions"] = json!([{"name": "decorate", "target": "application_upper", "parameters": ["string"], "returns": "string"}]);
    assert_eq!(names(&mut Session::new(&db), &plan(r).await).await, ["ADA"]);
}

#[tokio::test]
async fn application_controls_visibility_rollback_and_commit() {
    let db = database();
    let query = plan(request(
        "cypher",
        "MATCH (p:Person) RETURN p.name AS name ORDER BY name",
    ))
    .await;
    db.execute_batch("BEGIN; INSERT INTO people VALUES (4, 'Zoe')")
        .unwrap();
    {
        let mut session = Session::new(&db);
        assert_eq!(
            names(&mut session, &query).await,
            ["Ada", "Grace", ADVERSARIAL_NAME, "Zoe"]
        );
    }
    db.execute_batch("ROLLBACK").unwrap();
    assert_eq!(count(&db), 3);
    db.execute_batch("BEGIN; INSERT INTO people VALUES (4, 'Zoe')")
        .unwrap();
    assert_eq!(names(&mut Session::new(&db), &query).await.len(), 4);
    db.execute_batch("COMMIT").unwrap();
    assert_eq!(count(&db), 4);
}

#[tokio::test]
async fn dropping_a_partial_cursor_allows_session_reuse() {
    let db = database();
    let query = plan(request(
        "cypher",
        "MATCH (p:Person) RETURN p.name AS name ORDER BY name",
    ))
    .await;
    let mut session = Session::new(&db);
    {
        let mut cursor = execute(&mut session, &query).await.unwrap();
        assert_eq!(
            cursor
                .next()
                .unwrap()
                .unwrap()
                .column(0)
                .as_any()
                .downcast_ref::<StringArray>()
                .unwrap()
                .value(0),
            "Ada"
        );
        // Deliberately drop without advancing the reader to end-of-stream.
    }
    assert_eq!(
        names(&mut session, &query).await,
        ["Ada", "Grace", ADVERSARIAL_NAME]
    );
    assert_eq!(session.calls, 2);
}

#[tokio::test]
async fn invalid_dialect_and_protocol_never_reach_the_driver() {
    let db = database();
    let mut query = plan(request("cypher", "RETURN 'ok' AS name")).await;
    let mut session = Session::new(&db);
    query.dialect = "postgres".into();
    assert!(matches!(
        execute(&mut session, &query).await,
        Err(ExecutionError::Dialect { .. })
    ));
    query.dialect = "duckdb".into();
    query.version = 99;
    assert!(matches!(
        execute(&mut session, &query).await,
        Err(ExecutionError::Version(99))
    ));
    assert_eq!(session.calls, 0);
    assert_eq!(count(&db), 3);
}

#[tokio::test]
async fn unsupported_writes_and_invalid_metadata_fail_before_execution() {
    let db = database();
    for mut r in [
        request("cypher", "MATCH (p:Person) DELETE p"),
        request("cypher", "RETURN $missing"),
        request("cypher", "RETURN 1"),
    ] {
        if r["query"] == "RETURN 1" {
            r["nodes"][0]["id"] = json!("missing_column");
        }
        assert!(compile(serde_json::from_value(r).unwrap()).await.is_err());
    }
    assert_eq!(count(&db), 3);
}

#[tokio::test]
async fn driver_errors_propagate_without_fallback_and_connection_remains_owned() {
    let db = database();
    let query = plan(request("cypher", "MATCH (p:Person) RETURN p.name AS name")).await;
    db.execute_batch("DROP TABLE people").unwrap();
    let mut session = Session::new(&db);
    assert!(matches!(
        execute(&mut session, &query).await,
        Err(ExecutionError::Driver(_))
    ));
    assert_eq!(session.calls, 1);
    db.execute_batch(
        "CREATE TABLE people(id BIGINT, name VARCHAR); INSERT INTO people VALUES (1, 'Recovered')",
    )
    .unwrap();
    assert_eq!(names(&mut session, &query).await, ["Recovered"]);
    assert_eq!(session.calls, 2);
}

#[tokio::test]
async fn one_compiled_plan_can_be_routed_to_independent_engine_instances() {
    let first = database();
    let second = database();
    second
        .execute("UPDATE people SET name='Different' WHERE id=1", [])
        .unwrap();
    let query = plan(request(
        "cypher",
        "MATCH (p:Person) WHERE p.name <> 'Grace' RETURN p.name AS name ORDER BY name",
    ))
    .await;
    assert_eq!(
        names(&mut Session::new(&first), &query).await,
        ["Ada", ADVERSARIAL_NAME]
    );
    assert_eq!(
        names(&mut Session::new(&second), &query).await,
        ["Different", ADVERSARIAL_NAME]
    );
}

#[tokio::test]
async fn existing_view_is_used_without_materializing_or_changing_source_tables() {
    let db = database();
    db.execute_batch("CREATE VIEW selected_people AS SELECT * FROM people WHERE id <= 2")
        .unwrap();
    let mut r = request(
        "cypher",
        "MATCH (p:Person) RETURN p.name AS name ORDER BY name",
    );
    r["tables"][0]["name"] = json!("selected_people");
    r["nodes"][0]["table"] = json!("selected_people");
    let query = plan(r).await;
    assert_eq!(
        names(&mut Session::new(&db), &query).await,
        ["Ada", "Grace"]
    );
    assert_eq!(count(&db), 3);
    let tables: i64 = db
        .query_row(
            "SELECT count(*) FROM information_schema.tables WHERE table_schema='main'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        tables, 3,
        "compiler/executor must not create hidden materialized tables"
    );
}

#[tokio::test]
async fn arrow_schema_nulls_multiple_batches_and_retained_buffers() {
    let db = database();
    db.execute_batch("DELETE FROM follows; DELETE FROM people; INSERT INTO people SELECT i, CASE WHEN i % 7 = 0 THEN NULL ELSE 'person-' || i END FROM range(10000) t(i)").unwrap();
    let query = plan(request(
        "cypher",
        "MATCH (p:Person) RETURN p.name AS name ORDER BY p.id",
    ))
    .await;
    let mut session = Session::new(&db);
    let retained;
    {
        let mut reader = execute(&mut session, &query).await.unwrap();
        assert_eq!(reader.schema().field(0).name(), "name");
        retained = reader.next().unwrap().unwrap();
        let mut total = retained.num_rows();
        let mut batches = 1;
        for batch in reader {
            total += batch.unwrap().num_rows();
            batches += 1;
        }
        assert_eq!(total, 10000);
        assert!(batches > 1);
    }
    drop(session);
    drop(db);
    let values = retained
        .column(0)
        .as_any()
        .downcast_ref::<StringArray>()
        .unwrap();
    assert!(values.is_null(0));
    assert_eq!(values.value(1), "person-1");
}

#[tokio::test]
async fn statistics_protocol_retains_snapshot_and_preserves_diagnostics() {
    use orchiddb_client::Statistics;
    let r = request("cypher", "MATCH (p:Person) RETURN p.name AS name");
    let mut statistics = Statistics::default();
    let mut calls = 0;
    statistics
        .generate(r.clone(), |_work| {
            calls += 1;
            async { Err("Test adapter cannot bound this request".to_string()) }
        })
        .await
        .unwrap();
    assert!(calls > 0);
    assert!(statistics.snapshot().is_some());
    assert!(statistics.report().is_some());
    let typed = statistics.compile_plan(r.clone()).await.unwrap();
    assert!(typed.statistics_usage.is_some());
    let snapshot = statistics.snapshot().unwrap().clone();
    let with = statistics.compile(r.clone()).await.unwrap();
    assert!(with.get("statistics_usage").is_some());
    assert!(with.get("plan_estimates").is_some());
    statistics.clear().await.unwrap();
    assert!(statistics.snapshot().is_none());
    statistics.install(snapshot.clone()).await.unwrap();
    assert_eq!(statistics.snapshot(), Some(&snapshot));
    assert_eq!(statistics.compile(r).await.unwrap()["sql"], with["sql"]);
    statistics.clear().await.unwrap();
}

#[tokio::test]
async fn failed_statistics_generation_keeps_the_previous_snapshot() {
    use orchiddb_client::Statistics;
    let r = request("cypher", "MATCH (p:Person) RETURN p.name AS name");
    let mut statistics = Statistics::default();
    statistics
        .generate(r.clone(), |_| async {
            Err("Unsupported adapter".to_string())
        })
        .await
        .unwrap();
    let previous = statistics.snapshot().unwrap().clone();
    assert!(
        statistics
            .generate(r, |_| async { Ok(json!(null)) })
            .await
            .is_err()
    );
    assert_eq!(statistics.snapshot(), Some(&previous));
    statistics.clear().await.unwrap();
}

#[path = "../examples/support/duckdb_statistics.rs"]
mod statistics_adapter;

#[tokio::test]
async fn generated_statistics_use_the_existing_session_and_keep_all_languages_correct() {
    use orchiddb_client::Statistics;
    let db = database();
    let base = request(
        "cypher",
        "MATCH (p:Person) RETURN p.name AS name ORDER BY name",
    );
    let mut statistics = Statistics::default();
    statistics
        .generate(base, |work| {
            let result = statistics_adapter::collect_statistics(&db, &work);
            async move { result }
        })
        .await
        .unwrap();
    assert_eq!(
        statistics.snapshot().unwrap()["sources"]["people"]["sample_rows"],
        3
    );
    assert_eq!(
        statistics.snapshot().unwrap()["sources"]["follows"]["sample_rows"],
        2
    );
    assert!(
        statistics.report().unwrap()["skipped"]
            .as_object()
            .unwrap()
            .is_empty()
    );
    for (language, query) in [
        (
            "cypher",
            "MATCH (p:Person) RETURN p.name AS name ORDER BY name",
        ),
        ("gremlin", "g.V().hasLabel('Person').values('name').order()"),
        (
            "sparql",
            "SELECT ?name WHERE { ?p a <http://example.org/Person> ; <http://example.org/name> ?name . } ORDER BY ?name",
        ),
    ] {
        let plan = statistics.compile(request(language, query)).await.unwrap();
        let mut statement = db.prepare(plan["sql"].as_str().unwrap()).unwrap();
        let names = statement
            .query_map([], |row| row.get::<_, String>(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(names, ["Ada", "Grace", ADVERSARIAL_NAME]);
        assert!(plan.get("statistics_usage").is_some());
    }
    statistics.clear().await.unwrap();
    assert_eq!(count(&db), 3);
}

#[tokio::test]
async fn statistics_drop_and_cancel_release_native_capacity() {
    use orchiddb_client::Statistics;
    use std::{
        future::Future,
        task::{Context, Poll, Waker},
    };
    let r = request("cypher", "MATCH (p:Person) RETURN p.name AS name");
    // Exceed coordinator analysis capacity sequentially; cancelled futures must
    // release it even while an application callback is suspended.
    let mut statistics = Statistics::default();
    for _ in 0..20 {
        let future = statistics.generate(r.clone(), |_| async {
            std::future::pending::<Result<Value, String>>().await
        });
        let mut future = std::pin::pin!(future);
        assert!(matches!(
            future
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop())),
            Poll::Pending
        ));
    }
    // Exceed catalog capacity sequentially without explicitly calling clear.
    for _ in 0..70 {
        let mut temporary = Statistics::default();
        temporary
            .generate(r.clone(), |_| async {
                Err("Unsupported test session".to_string())
            })
            .await
            .unwrap();
    }
}

#[tokio::test]
async fn transport_truncation_is_not_reported_as_an_empty_table() {
    use orchiddb_client::Statistics;
    let db = database();
    let mut statistics = Statistics::default();
    statistics
        .generate(
            request("cypher", "MATCH (p:Person) RETURN p.name"),
            |mut work: Value| {
                if work["kind"] != "metadata" {
                    work["max_bytes"] = json!(1);
                }
                let result = statistics_adapter::collect_statistics(&db, &work);
                async move { result }
            },
        )
        .await
        .unwrap();
    let source = &statistics.snapshot().unwrap()["sources"]["people"];
    assert_eq!(source["sample_rows"], 0);
    assert_ne!(source["estimated_rows"], json!(0.0));
    assert_ne!(source["method"], "complete bounded read");
    assert_eq!(statistics.report().unwrap()["complete"], false);
}
