use super::*;
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::mpsc,
    thread,
};
fn serve(replies: Vec<(u16, Value)>) -> (String, mpsc::Receiver<String>, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let (tx, rx) = mpsc::channel();
    let handle = thread::spawn(move || {
        for (status, body) in replies {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut bytes = Vec::new();
            let mut buffer = [0u8; 4096];
            let mut length = None;
            loop {
                let n = stream.read(&mut buffer).unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&buffer[..n]);
                if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                    let headers = String::from_utf8_lossy(&bytes[..end]);
                    let content_length = headers
                        .lines()
                        .find_map(|line| {
                            line.to_ascii_lowercase()
                                .strip_prefix("content-length:")
                                .and_then(|s| s.trim().parse::<usize>().ok())
                        })
                        .unwrap_or(0);
                    length = Some(end + 4 + content_length);
                }
                if length.is_some_and(|len| bytes.len() >= len) {
                    break;
                }
            }
            tx.send(String::from_utf8(bytes).unwrap()).unwrap();
            let body = body.to_string();
            write!(stream,"HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
        }
    });
    (format!("http://{address}"), rx, handle)
}
fn column(name: &str, ty: &str, nullable: bool) -> TransferColumn {
    TransferColumn {
        name: name.into(),
        data_type: ty.into(),
        nullable,
    }
}
fn request(engine: &str) -> Value {
    json!({"version":1,"engine":engine,"operation":"search","api":"elastic","index":"docs","body":{"query":{"match_all":{}}},"columns":[{"source":"field","path":["id"]}],"limit":2})
}
#[test]
fn quickwit_score_uses_the_requested_sort_slot() {
    let session = HttpSession::new(
        Engine::Quickwit,
        HttpOptions::new("http://localhost:7280"),
        None,
    )
    .unwrap();
    let projections = vec![json!({"source":"score"})];
    assert!(
        session
            .score_projections(&projections, &json!({"sort":["id"]}))
            .is_err()
    );
    let projections = session
        .score_projections(
            &projections,
            &json!({"sort":[{"id":"asc"},{"_score":"desc"}]}),
        )
        .unwrap();
    let batch = decode_rows(
        &[json!({"sort":[42,1.25]})],
        &projections,
        &[column("score", "float64", false)],
        &[],
        false,
    )
    .unwrap();
    assert_eq!(
        ScalarValue::try_from_array(batch.column(0), 0).unwrap(),
        ScalarValue::Float64(Some(1.25))
    );
    assert!(
        decode_rows(
            &[json!({"_score":9.0})],
            &projections,
            &[column("score", "float64", true)],
            &[],
            false
        )
        .is_err()
    );
}
#[test]
fn typed_decode_preserves_json_null_missing_and_large_integers() {
    let columns = [column("id", "uint64", false), column("doc", "json", true)];
    let projections = json!([{"source":"field","path":["id"]},{"source":"field","path":["doc"]}]);
    let rows = json!([{"_source":{"id":18446744073709551615u64,"doc":null}},{"_source":{"id":9007199254740993u64}}]);
    let batch = decode_rows(
        rows.as_array().unwrap(),
        projections.as_array().unwrap(),
        &columns,
        &[],
        false,
    )
    .unwrap();
    assert_eq!(
        ScalarValue::try_from_array(batch.column(0), 0).unwrap(),
        ScalarValue::UInt64(Some(u64::MAX))
    );
    assert_eq!(
        crate::ir::functions::domain::json_text(
            &ScalarValue::try_from_array(batch.column(1), 0).unwrap()
        )
        .unwrap(),
        Some("null".into())
    );
    assert!(
        ScalarValue::try_from_array(batch.column(1), 1)
            .unwrap()
            .is_null()
    );
    assert!(decode_value(Some(&json!("1")), &DataType::Int64).is_err());
    assert!(decode_value(Some(&json!(1.5)), &DataType::Int64).is_err());
}
#[tokio::test]
async fn bounded_request_auth_and_schema_contract() {
    let (endpoint, requests, server) = serve(vec![(
        200,
        json!({"hits":{"hits":[{"_source":{"id":7}}]},"_shards":{"failed":0}}),
    )]);
    let mut options = HttpOptions::new(endpoint);
    options.authentication = Authentication::ApiKey("secret".into());
    let mut session = HttpSession::new(Engine::Elasticsearch, options, None).unwrap();
    let result = session
        .execute(&request("elasticsearch"), &[column("__c0", "int64", false)])
        .await
        .unwrap();
    assert_eq!(result[0].num_rows(), 1);
    let sent = requests.recv().unwrap();
    assert!(sent.starts_with("POST /docs/_search?allow_partial_search_results=false"));
    assert!(sent.to_lowercase().contains("authorization: apikey secret"));
    assert!(sent.contains("\"size\":2"));
    server.join().unwrap();
}
#[tokio::test]
async fn snapshot_pagination_consumes_all_pages_and_clears_context() {
    let (endpoint, requests, server) = serve(vec![
        (
            200,
            json!({"_scroll_id":"a","hits":{"hits":[{"_source":{"id":1}},{"_source":{"id":2}}]}}),
        ),
        (
            200,
            json!({"_scroll_id":"b","hits":{"hits":[{"_source":{"id":3}}]}}),
        ),
        (200, json!({"_scroll_id":"c","hits":{"hits":[]}})),
        (200, json!({"succeeded":true,"num_freed":1})),
    ]);
    let mut options = HttpOptions::new(endpoint);
    options.page_size = 2;
    let mut session = HttpSession::new(Engine::Elasticsearch, options, None).unwrap();
    let mut request = request("elasticsearch");
    request["limit"] = Value::Null;
    let batches = session
        .execute(&request, &[column("id", "int64", false)])
        .await
        .unwrap();
    assert_eq!(batches.iter().map(RecordBatch::num_rows).sum::<usize>(), 3);
    let sent = (0..4).map(|_| requests.recv().unwrap()).collect::<Vec<_>>();
    assert!(sent[0].contains("scroll=1m"));
    assert!(sent[1].starts_with("POST /_search/scroll"));
    assert!(sent[3].starts_with("DELETE /_search/scroll"));
    assert!(sent[3].contains("\"c\""));
    server.join().unwrap();
}
#[tokio::test]
async fn failed_first_scroll_page_is_rejected_and_released() {
    let (endpoint, requests, server) = serve(vec![
        (
            200,
            json!({"_scroll_id":"failed-cursor","_shards":{"failed":1},"hits":{"hits":[{"_source":{"id":1}}]}}),
        ),
        (200, json!({"succeeded":true,"num_freed":1})),
    ]);
    let mut session =
        HttpSession::new(Engine::Elasticsearch, HttpOptions::new(endpoint), None).unwrap();
    let mut request = request("elasticsearch");
    request["limit"] = Value::Null;
    assert!(
        session
            .execute(&request, &[column("id", "int64", false)])
            .await
            .unwrap_err()
            .contains("partial")
    );
    requests.recv().unwrap();
    let cleanup = requests.recv().unwrap();
    assert!(cleanup.starts_with("DELETE /_search/scroll"));
    assert!(cleanup.contains("failed-cursor"));
    server.join().unwrap();
}
#[tokio::test]
async fn explicit_bounds_and_zero_hit_aggregations_are_preserved() {
    let (endpoint, requests, server) = serve(vec![
        (200, json!({"hits":{"hits":[{"_source":{"id":7}}]}})),
        (
            200,
            json!({"hits":{"hits":[]},"aggregations":{"n":{"value":4}}}),
        ),
    ]);
    let mut session =
        HttpSession::new(Engine::Elasticsearch, HttpOptions::new(endpoint), None).unwrap();
    let mut query = request("elasticsearch");
    query["explicit"] = json!(true);
    query["body"]["from"] = json!(3);
    query["body"]["size"] = json!(1);
    let result = session
        .execute(&query, &[column("id", "int64", false)])
        .await
        .unwrap();
    assert_eq!(result[0].num_rows(), 1);
    let sent = requests.recv().unwrap();
    assert!(sent.contains("\"from\":3") && sent.contains("\"size\":1"));
    query["operation"] = json!("query");
    query["columns"] = json!([{"source":"response"}]);
    query["body"]["size"] = json!(0);
    query["limit"] = json!(0);
    let result = session
        .execute(&query, &[column("response", "json", false)])
        .await
        .unwrap();
    assert_eq!(result[0].num_rows(), 1);
    requests.recv().unwrap();
    server.join().unwrap();
}
#[tokio::test]
async fn failures_are_not_retried_or_returned_as_partial_rows() {
    for (status, body, expected) in [
        (429, json!({"error":{"reason":"busy"}}), "HTTP 429"),
        (200, json!({"timed_out":true,"hits":{"hits":[]}}), "partial"),
        (
            200,
            json!({"_shards":{"failed":1},"hits":{"hits":[]}}),
            "partial",
        ),
        (
            200,
            json!({"hits":{"hits":[{"_source":{"id":"wrong"}}]}}),
            "expected JSON number",
        ),
    ] {
        let (endpoint, requests, server) = serve(vec![(status, body)]);
        let mut session =
            HttpSession::new(Engine::Elasticsearch, HttpOptions::new(endpoint), None).unwrap();
        let error = session
            .execute(&request("elasticsearch"), &[column("id", "int64", false)])
            .await
            .unwrap_err();
        assert!(error.contains(expected), "{error}");
        assert!(requests.recv().is_ok());
        server.join().unwrap();
    }
}
#[tokio::test]
async fn native_query_returns_full_response_and_null_query_skips_http() {
    let (endpoint, requests, server) = serve(vec![(
        200,
        json!({"hits":[],"num_hits":0,"aggregations":{"n":{"value":7}}}),
    )]);
    let mut session = HttpSession::new(Engine::Quickwit, HttpOptions::new(endpoint), None).unwrap();
    let request = json!({"version":1,"engine":"quickwit","operation":"query","api":"native","index":"docs","body":{"query":"*","max_hits":0},"columns":[{"source":"response"}]});
    let output = session
        .execute(&request, &[column("result", "json", false)])
        .await
        .unwrap();
    let text = crate::ir::functions::domain::json_text(
        &ScalarValue::try_from_array(output[0].column(0), 0).unwrap(),
    )
    .unwrap()
    .unwrap();
    assert!(text.contains("aggregations"));
    assert!(
        requests
            .recv()
            .unwrap()
            .starts_with("POST /api/v1/docs/search")
    );
    server.join().unwrap();
    let mut request = super::tests::request("quickwit");
    request["body"] = json!({"query":null});
    request["null_query_paths"] = json!(["/body/query"]);
    assert_eq!(
        session
            .execute(&request, &[column("id", "int64", false)])
            .await
            .unwrap()[0]
            .num_rows(),
        0
    );
}
#[tokio::test]
async fn null_filter_guards_are_applied_without_changing_other_predicates() {
    let (endpoint, requests, server) = serve(vec![(200, json!({"hits":{"hits":[]}}))]);
    let mut session =
        HttpSession::new(Engine::Elasticsearch, HttpOptions::new(endpoint), None).unwrap();
    let mut request = request("elasticsearch");
    request["body"] = json!({"query":{"term":{"id":null}}});
    request["null_guards"] = json!([{"value":"/body/query/term/id","clause":"/body/query"}]);
    session
        .execute(&request, &[column("id", "int64", false)])
        .await
        .unwrap();
    assert!(requests.recv().unwrap().contains("match_none"));
    server.join().unwrap();
}
#[tokio::test]
async fn batched_requests_use_msearch_and_preserve_request_order() {
    let (endpoint, requests, server) = serve(vec![(
        200,
        json!({"responses":[{"hits":{"hits":[{"_source":{"id":10}}]}},{"hits":{"hits":[{"_source":{"id":20}}]}}]}),
    )]);
    let mut session =
        HttpSession::new(Engine::Elasticsearch, HttpOptions::new(endpoint), None).unwrap();
    let first = request("elasticsearch");
    let second = request("elasticsearch");
    let batches = session
        .execute_many(&[first, second], &[column("id", "int64", false)])
        .await
        .unwrap();
    assert_eq!(
        ScalarValue::try_from_array(batches[0][0].column(0), 0).unwrap(),
        ScalarValue::Int64(Some(10))
    );
    assert_eq!(
        ScalarValue::try_from_array(batches[1][0].column(0), 0).unwrap(),
        ScalarValue::Int64(Some(20))
    );
    let sent = requests.recv().unwrap();
    assert!(sent.starts_with("POST /_msearch"));
    assert!(sent.contains("application/x-ndjson"));
    assert_eq!(sent.split("\r\n\r\n").nth(1).unwrap().lines().count(), 4);
    server.join().unwrap();
}
#[tokio::test]
async fn batched_item_error_rejects_the_entire_result() {
    let (endpoint, requests, server) = serve(vec![(
        200,
        json!({"responses":[{"hits":{"hits":[{"_source":{"id":10}}]}},{"status":400,"error":{"reason":"unsupported query"}}]}),
    )]);
    let mut session =
        HttpSession::new(Engine::Elasticsearch, HttpOptions::new(endpoint), None).unwrap();
    assert!(
        session
            .execute_many(
                &[request("elasticsearch"), request("elasticsearch")],
                &[column("id", "int64", false)]
            )
            .await
            .unwrap_err()
            .contains("unsupported query")
    );
    assert!(requests.recv().is_ok());
    server.join().unwrap();
}
#[test]
fn parameter_null_flags_distinguish_sql_null_from_json_null() {
    let projections =
        json!([{"source":"parameter","parameter":0},{"source":"parameter","parameter":1}]);
    let projections =
        parameter_null_projections(projections.as_array().unwrap(), Some(&json!([false, true])))
            .unwrap();
    let batch = decode_rows(
        &[json!({"_source":{}})],
        &projections,
        &[
            column("json_null", "json", false),
            column("sql_null", "json", true),
        ],
        &[Value::Null, Value::Null],
        false,
    )
    .unwrap();
    assert!(
        !ScalarValue::try_from_array(batch.column(0), 0)
            .unwrap()
            .is_null()
    );
    assert!(
        ScalarValue::try_from_array(batch.column(1), 0)
            .unwrap()
            .is_null()
    );
}
#[test]
fn boolean_guards_preserve_unknown_under_not() {
    for value in [json!(true), json!(false), Value::Null] {
        let request = json!({"body":{"query":{"bool":{"must":[{},{}]}}},"boolean_guards":[{"parameter":0,"truth":true,"clause":"/body/query/bool/must/0"},{"parameter":0,"truth":false,"clause":"/body/query/bool/must/1"}]});
        let body = bound_body(&request, &[value.clone()]).unwrap();
        assert_eq!(
            body["query"]["bool"]["must"][0].get("match_all").is_some(),
            value == json!(true)
        );
        assert_eq!(
            body["query"]["bool"]["must"][1].get("match_all").is_some(),
            value == json!(false)
        );
    }
}
#[tokio::test]
async fn query_timeout_and_response_size_limits_are_enforced() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (_stream, _) = listener.accept().unwrap();
        thread::sleep(Duration::from_millis(100));
    });
    let mut options = HttpOptions::new(format!("http://{address}"));
    options.request_timeout = Duration::from_millis(15);
    let mut session = HttpSession::new(Engine::Elasticsearch, options, None).unwrap();
    assert!(
        session
            .execute(&request("elasticsearch"), &[column("id", "int64", false)])
            .await
            .unwrap_err()
            .contains("HTTP request failed")
    );
    server.join().unwrap();
    let (endpoint, requests, server) = serve(vec![(
        200,
        json!({"hits":{"hits":[]},"padding":"too large"}),
    )]);
    let mut options = HttpOptions::new(endpoint);
    options.max_response_bytes = 8;
    let mut session = HttpSession::new(Engine::Elasticsearch, options, None).unwrap();
    assert!(
        session
            .execute(&request("elasticsearch"), &[column("id", "int64", false)])
            .await
            .unwrap_err()
            .contains("byte limit")
    );
    assert!(requests.recv().is_ok());
    server.join().unwrap();
}
