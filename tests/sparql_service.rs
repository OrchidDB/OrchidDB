#![cfg(feature = "duckdb")]
use orchiddb::ir::rel::rdf::RdfDatasetMapping;
use orchiddb::ir::rel::sql::DuckDbExecutor;
use orchiddb::rdf_engine::{RdfGraphEngine, RdfTermValue, SparqlResults};
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::{Arc, mpsc},
    thread,
};

fn endpoint(status: &str, body: &str) -> (String, mpsc::Receiver<String>, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = format!("http://{}/sparql", listener.local_addr().unwrap());
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/sparql-results+json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let (send, receive) = mpsc::channel();
    let handle = thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        socket
            .set_read_timeout(Some(std::time::Duration::from_secs(20)))
            .unwrap();
        let mut data = Vec::new();
        let mut buffer = [0u8; 4096];
        loop {
            let size = socket.read(&mut buffer).unwrap();
            if size == 0 {
                break;
            }
            data.extend_from_slice(&buffer[..size]);
            if let Some(end) = data.windows(4).position(|bytes| bytes == b"\r\n\r\n") {
                let headers = String::from_utf8_lossy(&data[..end]);
                let length = headers
                    .lines()
                    .find_map(|line| {
                        line.to_ascii_lowercase()
                            .strip_prefix("content-length:")
                            .map(|value| value.trim().parse::<usize>().unwrap())
                    })
                    .unwrap();
                if data.len() >= end + 4 + length {
                    send.send(String::from_utf8(data[end + 4..].to_vec()).unwrap())
                        .unwrap();
                    break;
                }
            }
        }
        socket.write_all(response.as_bytes()).unwrap();
    });
    (address, receive, handle)
}

fn engine(url: String) -> RdfGraphEngine {
    let mut mapping = RdfDatasetMapping::new();
    mapping.map_service_endpoint("urn:remote", url);
    RdfGraphEngine::new(DuckDbExecutor::new(), Arc::new(mapping), "default")
}

#[tokio::test]
async fn external_arrow_bindings_join_local_sql_values() {
    let (url, request, server) = endpoint(
        "200 OK",
        r#"{"head":{"vars":["s","x"]},"results":{"bindings":[{"s":{"type":"uri","value":"urn:s"},"x":{"type":"literal","value":"2","datatype":"http://www.w3.org/2001/XMLSchema#integer"}}]}}"#,
    );
    let mut engine = engine(url);
    let sql = engine
        .sql("SELECT ?x WHERE { VALUES ?x { 1 2 } SERVICE <urn:remote> { ?s <urn:p> ?x } }")
        .await
        .unwrap();
    assert!(sql.contains("service"));
    assert!(
        matches!(request.try_recv(), Err(mpsc::TryRecvError::Empty)),
        "SQL planning must not contact the endpoint"
    );
    let actual = engine
        .query("SELECT ?x WHERE { VALUES ?x { 1 2 } SERVICE <urn:remote> { ?s <urn:p> ?x } }")
        .await
        .unwrap();
    assert_eq!(
        actual,
        SparqlResults::Solutions {
            variables: vec!["?x".into()],
            rows: vec![vec![Some(RdfTermValue::typed(
                "2",
                "http://www.w3.org/2001/XMLSchema#integer"
            ))]]
        }
    );
    let query = request.recv().unwrap();
    assert!(query.contains("<urn:p>"));
    assert!(!query.contains("VALUES"));
    orchiddb::language::sparql::parse_query(&query).unwrap();
    server.join().unwrap();
}

#[tokio::test]
async fn silent_failure_preserves_one_unbound_solution() {
    let (url, request, server) = endpoint("503 Service Unavailable", "unavailable");
    let mut engine = engine(url);
    let actual = engine
        .query(
            "SELECT ?x ?y WHERE { VALUES ?x { 1 } SERVICE SILENT <urn:remote> { ?s <urn:p> ?y } }",
        )
        .await
        .unwrap();
    assert_eq!(
        actual,
        SparqlResults::Solutions {
            variables: vec!["?x".into(), "?y".into()],
            rows: vec![vec![
                Some(RdfTermValue::typed(
                    "1",
                    "http://www.w3.org/2001/XMLSchema#integer"
                )),
                None
            ]]
        }
    );
    request.recv().unwrap();
    server.join().unwrap();
}

#[tokio::test]
async fn service_error_propagates_and_empty_result_stays_empty() {
    let (url, request, server) = endpoint("500 Internal Server Error", "failed");
    assert!(
        engine(url)
            .query("SELECT ?s WHERE { SERVICE <urn:remote> { ?s ?p ?o } }")
            .await
            .unwrap_err()
            .contains("SERVICE")
    );
    request.recv().unwrap();
    server.join().unwrap();
    let (url, request, server) = endpoint(
        "200 OK",
        r#"{"head":{"vars":["s"]},"results":{"bindings":[]}}"#,
    );
    assert_eq!(
        engine(url)
            .query("SELECT ?s WHERE { SERVICE <urn:remote> { ?s ?p ?o } }")
            .await
            .unwrap(),
        SparqlResults::Solutions {
            variables: vec!["?s".into()],
            rows: vec![]
        }
    );
    request.recv().unwrap();
    server.join().unwrap();
}
