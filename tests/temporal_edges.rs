//! Execute temporal edge lowering in real SQL engines, including Iceberg history.
use orchiddb::compiler::compile_json;
use serde_json::{Value, json};
use std::io::Write;
use std::process::{Command, Stdio};

fn request(dialect: &str, query: &str) -> Value {
    let columns = json!([{"name":"id","data_type":"int64"},{"name":"symbol","data_type":"string"},{"name":"ts","data_type":"timestamp"}]);
    json!({"version":1,"dialect":dialect,"language":"cypher","query":query,
        "tables":[{"name":"events","columns":columns},{"name":"quotes","columns":columns}],
        "nodes":[{"label":"Event","table":"events","id":"id","properties":{"id":"id","symbol":"symbol","ts":"ts"}},
                 {"label":"Quote","table":"quotes","id":"id","properties":{"id":"id","symbol":"symbol","ts":"ts"}}],
        "computed_relationships":[{"name":"PRICE","source":"Event","target":"Quote",
            "as_of":{"source_time":"ts","target_time":"ts","partition_by":{"symbol":"symbol"}}}]})
}
async fn sql(request: &Value) -> String {
    let result: Value =
        serde_json::from_str(&compile_json(&request.to_string()).await.unwrap()).unwrap();
    result["sql"].as_str().unwrap().to_owned()
}
fn python(script: &str, input: &Value) -> Value {
    let python = std::env::var("ORCHIDDB_TEST_PYTHON")
        .unwrap_or_else(|_| "extension/vendor/test-env/bin/python".into());
    let mut child = Command::new(python)
        .args(["-c", script])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.to_string().as_bytes())
        .unwrap();
    let result = child.wait_with_output().unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    serde_json::from_slice(&result.stdout).unwrap()
}
fn trino(sql: &str) -> Vec<Value> {
    let result = Command::new("python3")
        .args(["tools/trino/query.py", sql])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{sql}\n{}",
        String::from_utf8_lossy(&result.stderr)
    );
    String::from_utf8(result.stdout)
        .unwrap()
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}
const EVENT_ROWS: &str = "(1,'A',TIMESTAMP '2020-01-01 10:00:00'),(2,'A',TIMESTAMP '2020-01-01 11:00:00'),(3,'B',TIMESTAMP '2020-01-01 11:00:00'),(4,'C',TIMESTAMP '2020-01-01 11:00:00'),(5,'A',CAST(NULL AS TIMESTAMP)),(6,'A',TIMESTAMP '2020-01-01 08:00:00')";
const QUOTE_ROWS: &str = "(10,'A',TIMESTAMP '2020-01-01 09:00:00'),(20,'A',TIMESTAMP '2020-01-01 10:00:00'),(21,'A',TIMESTAMP '2020-01-01 10:00:00'),(30,'A',TIMESTAMP '2020-01-01 12:00:00'),(40,'B',TIMESTAMP '2020-01-01 10:30:00'),(50,'A',CAST(NULL AS TIMESTAMP))";
fn fixture(sql: &str, quotes: bool) -> String {
    let mut ctes = format!("events(id,symbol,ts) AS (VALUES {EVENT_ROWS})");
    if quotes {
        ctes.push_str(&format!(", quotes(id,symbol,ts) AS (VALUES {QUOTE_ROWS})"));
    }
    if let Some(rest) = sql.strip_prefix("WITH ") {
        format!("WITH {ctes}, {rest}")
    } else {
        format!("WITH {ctes} {sql}")
    }
}
#[tokio::test]
async fn asof_matching_duckdb_and_trino() {
    for dialect in ["duckdb", "trino"] {
        for optional in [false, true] {
            let query = if optional {
                "MATCH (e:Event) OPTIONAL MATCH (e)-[:PRICE]->(q:Quote) RETURN e.id AS event, q.id AS quote ORDER BY event"
            } else {
                "MATCH (e:Event)-[:PRICE]->(q:Quote) RETURN e.id AS event, q.id AS quote ORDER BY event"
            };
            let lowered = sql(&request(dialect, query)).await;
            let query = fixture(&lowered, true);
            let expected = if optional {
                json!([[1, 20], [2, 20], [3, 40], [4, null], [5, null], [6, null]])
            } else {
                json!([[1, 20], [2, 20], [3, 40]])
            };
            if dialect == "duckdb" {
                assert_eq!(
                    python(
                        "import duckdb,json,sys; print(json.dumps(duckdb.connect().execute(json.load(sys.stdin)).fetchall()))",
                        &json!(query)
                    ),
                    expected,
                    "{query}"
                );
            } else if std::env::var_os("ORCHIDDB_TEST_TRINO").is_some() {
                let rows = trino(&query)
                    .into_iter()
                    .map(|r| json!([r["event"], r["quote"]]))
                    .collect::<Vec<_>>();
                assert_eq!(json!(rows), expected, "{query}");
            }
        }
    }
}
#[tokio::test]
async fn validates_temporal_schema_and_snapshot_options() {
    let mut r = request("duckdb", "MATCH (e:Event)-[:PRICE]->(q:Quote) RETURN q.id");
    r["tables"][1]["columns"][2]["data_type"] = json!("string");
    assert!(
        compile_json(&r.to_string())
            .await
            .unwrap_err()
            .contains("matching declared types")
    );
    let mut r = request("trino", "MATCH (q:Quote) RETURN q.id");
    r["source_metadata"] = json!([{"table":"quotes","format":"trino","options":{"connector":"iceberg","catalog":"iceberg","schema":"orchid","table":"quotes","snapshot":1,"timestamp_as_of":"2020-01-01T00:00:00Z"}}]);
    assert!(
        compile_json(&r.to_string())
            .await
            .unwrap_err()
            .contains("mutually exclusive")
    );
    r["source_metadata"][0]["options"]
        .as_object_mut()
        .unwrap()
        .remove("snapshot");
    r["source_metadata"][0]["options"]["timestamp_as_of"] =
        json!("2020-01-01'); DROP TABLE quotes; --");
    assert!(
        compile_json(&r.to_string())
            .await
            .unwrap_err()
            .contains("RFC3339")
    );
}
#[tokio::test]
async fn iceberg_timestamp_snapshot_duckdb() {
    let root = std::env::current_dir()
        .unwrap()
        .join("target")
        .join(format!("temporal-iceberg-{}", std::process::id()));
    let metadata = python(
        r#"
import sys,json,pathlib,datetime,time
import pyarrow as pa
from pyiceberg.catalog.sql import SqlCatalog
root=pathlib.Path(json.load(sys.stdin)); root.mkdir(parents=True,exist_ok=True)
c=SqlCatalog('temporal',uri='sqlite:///'+str(root/'catalog.db'),warehouse=root.as_uri())
c.create_namespace_if_not_exists('fixture')
name='fixture.quotes_'+str(time.time_ns())
schema=pa.schema([('id',pa.int64()),('symbol',pa.string()),('ts',pa.timestamp('us'))])
t=c.create_table(name,schema=schema)
def append(i,h):t.append(pa.table({'id':[i],'symbol':['A'],'ts':[datetime.datetime(2020,1,1,h)]},schema=schema))
append(10,9)
stamp=datetime.datetime.fromtimestamp(t.current_snapshot().timestamp_ms/1000,datetime.timezone.utc).isoformat()
append(20,10)
print(json.dumps({'path':t.metadata_location,'timestamp':stamp}))
"#,
        &json!(root.to_str().unwrap()),
    );
    let mut r = request(
        "duckdb",
        "MATCH (e:Event)-[:PRICE]->(q:Quote) RETURN e.id AS event, q.id AS quote ORDER BY event",
    );
    r["source_metadata"] = json!([{"table":"quotes","format":"duckdb_iceberg","options":{"path":metadata["path"],"timestamp_as_of":metadata["timestamp"]}}]);
    let historical = sql(&r).await;
    assert!(
        historical.contains("snapshot_from_timestamp"),
        "{historical}"
    );
    r["source_metadata"][0]["options"]
        .as_object_mut()
        .unwrap()
        .remove("timestamp_as_of");
    let current = sql(&r).await;
    let rows = python(
        "import duckdb,json,sys; q=json.load(sys.stdin); c=duckdb.connect(); c.execute('INSTALL iceberg; LOAD iceberg'); print(json.dumps([c.execute(s).fetchall() for s in q]))",
        &json!([fixture(&historical, false), fixture(&current, false)]),
    );
    assert_eq!(rows, json!([[[1, 10], [2, 10]], [[1, 20], [2, 20]]]));
}
#[tokio::test]
async fn iceberg_timestamp_snapshot_trino() {
    if std::env::var_os("ORCHIDDB_TEST_TRINO").is_none() {
        return;
    }
    let name = format!("temporal_quotes_{}", std::process::id());
    let table = format!("iceberg.orchid.{name}");
    trino(&format!("DROP TABLE IF EXISTS {table}"));
    trino(&format!(
        "CREATE TABLE {table} (id BIGINT, symbol VARCHAR, ts TIMESTAMP(6))"
    ));
    trino(&format!(
        "INSERT INTO {table} VALUES (10,'A',TIMESTAMP '2020-01-01 09:00:00')"
    ));
    let timestamp=trino(&format!("SELECT to_iso8601(committed_at) AS ts FROM iceberg.orchid.\"{name}$snapshots\" ORDER BY committed_at DESC LIMIT 1"))[0]["ts"].clone();
    trino(&format!(
        "INSERT INTO {table} VALUES (20,'A',TIMESTAMP '2020-01-01 10:00:00')"
    ));
    let mut r = request(
        "trino",
        "MATCH (e:Event)-[:PRICE]->(q:Quote) RETURN e.id AS event, q.id AS quote ORDER BY event",
    );
    r["source_metadata"] = json!([{"table":"quotes","format":"trino","options":{"connector":"iceberg","catalog":"iceberg","schema":"orchid","table":name,"timestamp_as_of":timestamp}}]);
    let historical = sql(&r).await;
    assert!(historical.contains("FOR TIMESTAMP AS OF"), "{historical}");
    let rows = trino(&fixture(&historical, false));
    assert_eq!(
        rows,
        vec![json!({"event":1,"quote":10}), json!({"event":2,"quote":10})]
    );
    r["source_metadata"][0]["options"]
        .as_object_mut()
        .unwrap()
        .remove("timestamp_as_of");
    let rows = trino(&fixture(&sql(&r).await, false));
    assert_eq!(
        rows,
        vec![json!({"event":1,"quote":20}), json!({"event":2,"quote":20})]
    );
    trino(&format!("DROP TABLE {table}"));
}

#[test]
fn temporal_schema_roundtrips_before_sources_are_bound() {
    use orchiddb::ir::rel::mapping::GraphMapping;
    let mapping = GraphMapping::from_toml(
        r#"
[node.Event]
table="events"
id="id"
[node.Event.properties]
ts="ts"
symbol="symbol"
[node.Quote]
table="quotes"
id="id"
[node.Quote.properties]
ts="ts"
symbol="symbol"
[edge.PRICE]
source="Event"
target="Quote"
[edge.PRICE.as_of]
source_time="ts"
target_time="ts"
partition_by={symbol="symbol"}
"#,
    )
    .unwrap();
    let encoded = mapping.try_to_toml().unwrap();
    assert!(encoded.contains("as_of"));
    assert_eq!(
        GraphMapping::from_toml(&encoded)
            .unwrap()
            .try_to_toml()
            .unwrap(),
        encoded
    );
}
