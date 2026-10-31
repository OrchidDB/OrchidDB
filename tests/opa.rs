#![cfg(feature = "opa")]
use orchiddb::{
    compiler::{CompileRequest, compile},
    opa::{Client, Config, Identity, Policy},
};
use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader, Read, Write},
    net::TcpListener,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread,
    time::Duration,
};

struct Reply {
    status: u16,
    body: String,
    delay: Duration,
}
impl Reply {
    fn result(value: Value) -> Self {
        Self {
            status: 200,
            body: json!({"result":value}).to_string(),
            delay: Duration::ZERO,
        }
    }
}
struct Server {
    url: String,
    calls: Arc<Mutex<Vec<Value>>>,
    stop: Arc<AtomicBool>,
    peak: Arc<AtomicUsize>,
    thread: Option<thread::JoinHandle<()>>,
}
impl Server {
    fn new(handler: impl Fn(&Value, &str) -> Reply + Send + Sync + 'static) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        listener.set_nonblocking(true).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let calls = Arc::new(Mutex::new(Vec::new()));
        let active = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));
        let handler = Arc::new(handler);
        let (done, log, max) = (stop.clone(), calls.clone(), peak.clone());
        let thread = thread::spawn(move || {
            while !done.load(Ordering::SeqCst) {
                let Ok((mut socket, _)) = listener.accept() else {
                    thread::sleep(Duration::from_millis(1));
                    continue;
                };
                let (handler, log, active, max) =
                    (handler.clone(), log.clone(), active.clone(), max.clone());
                thread::spawn(move || {
                    socket.set_nonblocking(false).unwrap();
                    socket
                        .set_read_timeout(Some(Duration::from_secs(5)))
                        .unwrap();
                    let mut input = BufReader::new(socket.try_clone().unwrap());
                    let mut first = String::new();
                    if input.read_line(&mut first).is_err() {
                        return;
                    }
                    let path = first.split_whitespace().nth(1).unwrap_or("").to_owned();
                    let mut length = 0;
                    loop {
                        let mut header = String::new();
                        if input.read_line(&mut header).unwrap_or(0) == 0 {
                            return;
                        }
                        if header == "\r\n" {
                            break;
                        }
                        if let Some(value) = header.to_lowercase().strip_prefix("content-length:") {
                            length = value.trim().parse().unwrap();
                        }
                    }
                    let mut bytes = vec![0; length];
                    input.read_exact(&mut bytes).unwrap();
                    let value: Value = serde_json::from_slice(&bytes).unwrap();
                    log.lock().unwrap().push(value.clone());
                    let count = active.fetch_add(1, Ordering::SeqCst) + 1;
                    max.fetch_max(count, Ordering::SeqCst);
                    let reply = handler(&value, &path);
                    thread::sleep(reply.delay);
                    let _ = write!(
                        socket,
                        "HTTP/1.1 {} Reply\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        reply.status,
                        reply.body.len(),
                        reply.body
                    );
                    active.fetch_sub(1, Ordering::SeqCst);
                });
            }
        });
        Self {
            url,
            calls,
            stop,
            peak,
            thread: Some(thread),
        }
    }
    fn config(&self) -> Config {
        Config::new(format!("{}/allow", self.url))
    }
    fn policy(&self, config: Config) -> Policy {
        Policy::new(
            Arc::new(Client::new(config).unwrap()),
            Identity::new("alice"),
        )
        .unwrap()
    }
    fn calls(&self) -> Vec<Value> {
        self.calls.lock().unwrap().clone()
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        self.thread.take().unwrap().join().unwrap();
    }
}
fn request(query: &str, policy: Option<Policy>) -> CompileRequest {
    let mut request: CompileRequest = serde_json::from_value(json!({"version":1,"dialect":"duckdb","language":"cypher","query":query,
        "tables":[{"name":"people","columns":[
            {"name":"id","data_type":"int64","nullable":false},
            {"name":"name","data_type":"string"},
            {"name":"salary","data_type":"int64"},
            {"name":"tenant","data_type":"string"},
            {"name":"unused","data_type":"string"}]}],
        "nodes":[{"label":"Person","table":"people","id":"id","properties":{"name":"name","salary":"salary","tenant":"tenant","unused":"unused"}}]
    })).unwrap();
    request.opa = policy;
    request
}
fn operation(input: &Value) -> &str {
    input["input"]["action"]["operation"].as_str().unwrap()
}
fn execute(sql: &str) -> Value {
    let setup = "CREATE TABLE people(id BIGINT NOT NULL, name VARCHAR, salary BIGINT, tenant VARCHAR, unused VARCHAR); INSERT INTO people VALUES (1,'Alice',10,'a','secret'), (2,'Bob',20,'b','secret'), (3,'Carol',30,'a','secret');";
    let output = std::process::Command::new("duckdb")
        .args(["-json", ":memory:", &format!("{setup} {sql}")])
        .output()
        .expect("DuckDB CLI is required for OPA execution regression tests");
    assert!(
        output.status.success(),
        "{}\n{sql}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|_| panic!("{}\n{sql}", String::from_utf8_lossy(&output.stdout)))
}
#[tokio::test]
async fn disabled_path_has_no_policy_requests_and_allow_is_query_scoped() {
    let server = Server::new(|_, _| Reply::result(json!(true)));
    compile(request("MATCH (p:Person) RETURN p.name", None))
        .await
        .unwrap();
    assert!(server.calls().is_empty());
    let policy = server.policy(server.config());
    for _ in 0..2 {
        compile(request(
            "MATCH (p:Person) RETURN p.name",
            Some(policy.clone()),
        ))
        .await
        .unwrap();
    }
    let calls = server.calls();
    assert_eq!(calls.len(), 4);
    assert_ne!(
        calls[0]["input"]["context"]["queryId"],
        calls[2]["input"]["context"]["queryId"]
    );
    assert_eq!(calls[0]["input"]["context"]["identity"]["user"], "alice");
    let columns = calls[1]["input"]["action"]["resource"]["table"]["columns"]
        .as_array()
        .unwrap();
    assert!(columns.contains(&json!("name")));
    assert!(!columns.contains(&json!("unused")), "{calls:?}");
}
#[tokio::test]
async fn row_filters_masks_and_user_predicates_execute_in_order() {
    let server = Server::new(|input, path| match path {
        "/rows" => Reply::result(json!([{"expression":"tenant = 'a'"}])),
        "/masks" => {
            let columns = input["input"]["action"]["filterResources"]
                .as_array()
                .unwrap();
            Reply::result(Value::Array(
                columns
                    .iter()
                    .enumerate()
                    .filter(|(_, r)| r["column"]["columnName"] == "salary")
                    .map(|(i, _)| json!({"index":i,"viewExpression":{"expression":"0"}}))
                    .collect(),
            ))
        }
        _ => Reply::result(json!(true)),
    });
    let mut config = server.config();
    config.row_filters_uri = Some(format!("{}/rows", server.url));
    config.batch_column_masking_uri = Some(format!("{}/masks", server.url));
    let policy = server.policy(config);
    let sql = compile(request("MATCH (p:Person) WHERE p.salary = 0 RETURN p.name AS name, p.salary AS salary ORDER BY name", Some(policy))).await.unwrap().sql;
    assert_eq!(
        execute(&sql),
        json!([{"name":"Alice","salary":0},{"name":"Carol","salary":0}])
    );
    assert_eq!(server.calls().len(), 4);
}
#[tokio::test]
async fn batch_requests_deduplicate_scans_and_preserve_optional_matches() {
    let server = Server::new(|input, path| match path {
        "/batch" => Reply::result(json!(
            (0..input["input"]["action"]["filterResources"]
                .as_array()
                .unwrap()
                .len())
                .collect::<Vec<_>>()
        )),
        "/rows" => Reply::result(json!([{"expression":"tenant = 'a'"}])),
        "/masks" => Reply::result(json!([])),
        _ => Reply::result(json!(true)),
    });
    let mut config = server.config();
    config.batched_uri = Some(format!("{}/batch", server.url));
    config.row_filters_uri = Some(format!("{}/rows", server.url));
    config.batch_column_masking_uri = Some(format!("{}/masks", server.url));
    let sql=compile(request("MATCH (a:Person) WHERE a.name = 'Alice' OPTIONAL MATCH (b:Person) WHERE b.name = 'Bob' RETURN a.name AS name, b.name AS other",Some(server.policy(config)))).await.unwrap().sql;
    assert_eq!(execute(&sql), json!([{"name":"Alice","other":null}]));
    assert_eq!(
        server.calls().len(),
        4,
        "same table must be authorized only once"
    );
}
#[tokio::test]
async fn invalid_decisions_fail_closed() {
    for body in [
        "{}",
        "null",
        "not json",
        "{\"result\":false}",
        "{\"result\":null}",
        "{\"result\":\"true\"}",
    ] {
        let body = body.to_owned();
        let server = Server::new(move |_, _| Reply {
            status: 200,
            body: body.clone(),
            delay: Duration::ZERO,
        });
        assert!(
            compile(request("RETURN 1", Some(server.policy(server.config()))))
                .await
                .unwrap_err()
                .contains("OPA:")
        );
    }
    let server = Server::new(|_, _| Reply {
        status: 503,
        body: "unavailable".into(),
        delay: Duration::ZERO,
    });
    assert!(
        compile(request("RETURN 1", Some(server.policy(server.config()))))
            .await
            .unwrap_err()
            .contains("503")
    );
}
#[tokio::test]
async fn invalid_filters_masks_and_indices_fail_closed() {
    for (path, result) in [
        ("rows", json!([{"expression":"1"}])),
        ("rows", json!([{"expression":"true; SELECT 1"}])),
        ("rows", json!([{"expression":"true","identity":"admin"}])),
        ("rows", Value::Null),
        (
            "masks",
            json!([{"index":999,"viewExpression":{"expression":"NULL"}}]),
        ),
        (
            "masks",
            json!([{"index":0,"viewExpression":{"expression":"NULL"}},{"index":0,"viewExpression":{"expression":"NULL"}}]),
        ),
        ("batch", json!([])),
        ("batch", json!([0, 0])),
        ("batch", json!([1])),
    ] {
        let server = Server::new(move |_, uri| {
            if uri == format!("/{path}") {
                Reply::result(result.clone())
            } else {
                Reply::result(json!(true))
            }
        });
        let mut config = server.config();
        let endpoint = Some(format!("{}/{path}", server.url));
        match path {
            "rows" => config.row_filters_uri = endpoint,
            "masks" => config.batch_column_masking_uri = endpoint,
            _ => config.batched_uri = endpoint,
        };
        assert!(
            compile(request(
                "MATCH (p:Person) RETURN p.name",
                Some(server.policy(config))
            ))
            .await
            .is_err(),
            "{path}"
        );
    }
}
#[tokio::test]
async fn transport_limits_and_bounded_parallel_fallback() {
    let server = Server::new(|input, _| {
        let mut result = Reply::result(if operation(input) == "GetColumnMask" {
            Value::Null
        } else {
            json!(true)
        });
        result.delay = Duration::from_millis(30);
        result
    });
    let mut config = server.config();
    config.max_concurrent_requests = 2;
    config.column_masking_uri = Some(format!("{}/mask", server.url));
    compile(request(
        "MATCH (p:Person) RETURN p.name,p.salary,p.tenant,p.unused",
        Some(server.policy(config)),
    ))
    .await
    .unwrap();
    assert_eq!(
        server.peak.load(Ordering::SeqCst),
        2,
        "fallback must run concurrently within its bound"
    );
    let mut config = server.config();
    config.timeout = Duration::from_millis(5);
    assert!(
        compile(request("RETURN 1", Some(server.policy(config))))
            .await
            .unwrap_err()
            .contains("timed out")
    );
    let mut config = server.config();
    config.max_response_bytes = 3;
    assert!(
        compile(request("RETURN 1", Some(server.policy(config))))
            .await
            .unwrap_err()
            .contains("size limit")
    );
}
#[tokio::test]
async fn null_masks_affect_aggregates_and_projection_only_columns() {
    let server = Server::new(|_, path| {
        if path == "/mask" {
            Reply::result(json!({"expression":"NULL"}))
        } else {
            Reply::result(json!(true))
        }
    });
    let mut config = server.config();
    config.column_masking_uri = Some(format!("{}/mask", server.url));
    let sql = compile(request(
        "MATCH (p:Person) RETURN count(p.salary) AS n",
        Some(server.policy(config)),
    ))
    .await
    .unwrap()
    .sql;
    assert_eq!(execute(&sql), json!([{"n":0}]));
}

#[tokio::test]
async fn batches_scale_with_tables_instead_of_columns() {
    let mut timings = Vec::new();
    for batched in [false, true] {
        let server = Server::new(|input, _| {
            let mut response = if operation(input) == "GetColumnMask" {
                Reply::result(if input["input"]["action"]["filterResources"].is_array() {
                    json!([])
                } else {
                    Value::Null
                })
            } else {
                Reply::result(json!(true))
            };
            response.delay = Duration::from_millis(5);
            response
        });
        let mut config = server.config();
        if batched {
            config.batch_column_masking_uri = Some(format!("{}/masks", server.url));
        } else {
            config.column_masking_uri = Some(format!("{}/mask", server.url));
        }
        let mut req = request("RETURN 1", Some(server.policy(config)));
        for index in 0..64 {
            let name = format!("c{index}");
            let column = orchiddb::compiler::Column {
                name: name.clone(),
                data_type: "string".into(),
                nullable: true,
            };
            req.tables[0].columns.push(column);
            req.nodes[0].properties.insert(name.clone(), name);
        }
        req.query = format!(
            "MATCH (p:Person) RETURN {}",
            (0..64)
                .map(|i| format!("p.c{i}"))
                .collect::<Vec<_>>()
                .join(",")
        );
        let start = std::time::Instant::now();
        compile(req).await.unwrap();
        let count = server.calls().len();
        assert_eq!(count, if batched { 3 } else { 66 });
        timings.push((batched, count, start.elapsed()));
    }
    eprintln!("64-column policy workload (5ms mock service latency): {timings:?}");
}

#[test]
fn policy_configuration_cannot_be_supplied_in_query_json() {
    let req = request("RETURN 1", None);
    let mut value = serde_json::to_value(req).unwrap();
    value["opa"] = json!({"policy_uri":"http://attacker/allow"});
    assert!(serde_json::from_value::<CompileRequest>(value).is_err());
}

#[tokio::test]
async fn real_opa_policy_execution() {
    let Ok(url) = std::env::var("ORCHIDDB_TEST_OPA_URL") else {
        return;
    };
    let mut config = Config::new(format!("{url}/v1/data/orchid_test/allow"));
    config.batched_uri = Some(format!("{url}/v1/data/orchid_test/batch"));
    config.row_filters_uri = Some(format!("{url}/v1/data/orchid_test/rowFilters"));
    config.batch_column_masking_uri = Some(format!("{url}/v1/data/orchid_test/batchMasks"));
    let client = Arc::new(Client::new(config).unwrap());
    let policy = Policy::new(client.clone(), Identity::new("alice")).unwrap();
    let compiled = compile(request(
        "MATCH (p:Person) WHERE p.salary = 0 RETURN p.name AS name ORDER BY name",
        Some(policy),
    ))
    .await
    .unwrap();
    assert_eq!(
        execute(&compiled.sql),
        json!([{"name":"Alice"},{"name":"Carol"}])
    );
    let denied = Policy::new(client, Identity::new("mallory")).unwrap();
    assert!(
        compile(request("MATCH (p:Person) RETURN p.name", Some(denied)))
            .await
            .unwrap_err()
            .contains("access denied")
    );
}

#[tokio::test]
async fn query_denial_precedes_frontend_planning() {
    let server = Server::new(|_, _| Reply::result(json!(false)));
    let error = compile(request(
        "deliberately invalid query",
        Some(server.policy(server.config())),
    ))
    .await
    .unwrap_err();
    assert!(error.contains("OPA: access denied"), "{error}");
    assert_eq!(server.calls().len(), 1);
}

#[tokio::test]
async fn masks_override_non_null_source_metadata() {
    let server = Server::new(|_, path| {
        if path == "/mask" {
            Reply::result(json!({"expression":"NULL"}))
        } else {
            Reply::result(json!(true))
        }
    });
    let mut config = server.config();
    config.column_masking_uri = Some(format!("{}/mask", server.url));
    let mut req = request(
        "MATCH (p:Person) RETURN count(p.key) AS n",
        Some(server.policy(config)),
    );
    req.nodes[0].properties.insert("key".into(), "id".into());
    let sql = compile(req).await.unwrap().sql;
    assert_eq!(execute(&sql), json!([{"n":0}]));
}

#[tokio::test]
async fn qualified_sources_and_predicate_only_columns_are_authorized() {
    let server = Server::new(|input, _| {
        if operation(input) == "SelectFromColumns" {
            let table = &input["input"]["action"]["resource"]["table"];
            assert_eq!(table["catalogName"], "warehouse");
            assert_eq!(table["schemaName"], "private");
            assert_eq!(table["tableName"], "people");
            assert!(
                table["columns"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("salary"))
            );
            return Reply::result(json!(false));
        }
        Reply::result(json!(true))
    });
    let mut req = request(
        "MATCH (p:Person) WHERE p.salary > 0 RETURN p.name",
        Some(server.policy(server.config())),
    );
    req.tables[0].name = "warehouse.private.people".into();
    req.nodes[0].table = req.tables[0].name.clone();
    assert!(compile(req).await.unwrap_err().contains("access denied"));
    assert_eq!(server.calls().len(), 2);
}

#[tokio::test]
async fn declared_table_functions_share_the_policy_boundary() {
    let server = Server::new(|input, path| {
        if path == "/mask"
            && input["input"]["action"]["resource"]["column"]["columnName"] == "salary"
        {
            return Reply::result(json!({"expression":"0"}));
        }
        Reply::result(if path == "/mask" {
            Value::Null
        } else {
            json!(true)
        })
    });
    let mut config = server.config();
    config.column_masking_uri = Some(format!("{}/mask", server.url));
    let mut req = request(
        "MATCH (p:Person) WHERE p.tenant = 'a' AND p.salary = 0 RETURN p.name AS name, p.salary AS salary",
        Some(server.policy(config)),
    );
    req.source_metadata.push(
        serde_json::from_value(
            json!({"table":"people","format":"table_function","options":{
                "function":["rows_for_tenant"],"arguments":[{"column":"tenant"}]
            }}),
        )
        .unwrap(),
    );
    let sql = compile(req).await.unwrap().sql;
    let setup = "CREATE MACRO rows_for_tenant(t) AS TABLE SELECT 1::BIGINT AS id, 'Alice' AS name, 10::BIGINT AS salary, 'hidden' AS unused;";
    assert_eq!(
        execute(&format!("{setup} {sql}")),
        json!([{"name":"Alice","salary":0}])
    );
}
