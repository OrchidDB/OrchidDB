#![cfg(feature = "opa")]
use orchiddb_client::arrow::{
    datatypes::Schema as ArrowSchema,
    error::ArrowError,
    record_batch::{RecordBatch, RecordBatchIterator},
};
use orchiddb_client::{Connection, Query, Schema, SqlDialect, SqlSession, SqlWork, opa};
use std::{
    io::{BufRead, BufReader, Read, Write},
    net::TcpListener,
    sync::Arc,
};

#[derive(Default)]
struct Session {
    executions: usize,
}
impl SqlSession for Session {
    type Error = String;
    type Output<'a> = RecordBatchIterator<std::vec::IntoIter<Result<RecordBatch, ArrowError>>>;
    fn dialect(&self) -> SqlDialect {
        SqlDialect::DuckDb
    }
    async fn query<'a>(&'a mut self, _: &SqlWork) -> Result<Self::Output<'a>, String> {
        self.executions += 1;
        Ok(RecordBatchIterator::new(
            Vec::new().into_iter(),
            Arc::new(ArrowSchema::empty()),
        ))
    }
}
#[tokio::test]
async fn connection_enforces_host_identity_before_any_database_execution() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let uri = format!("http://{}/allow", listener.local_addr().unwrap());
    let server = std::thread::spawn(move || {
        for _ in 0..2 {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                .unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut length = 0;
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                if line == "\r\n" {
                    break;
                }
                if let Some(value) = line.to_lowercase().strip_prefix("content-length:") {
                    length = value.trim().parse().unwrap();
                }
            }
            let mut body = vec![0; length];
            reader.read_exact(&mut body).unwrap();
            let request: serde_json::Value = serde_json::from_slice(&body).unwrap();
            assert_eq!(request["input"]["context"]["identity"]["user"], "alice");
            let body = r#"{"result":false}"#;
            write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body).unwrap();
        }
    });
    let client = Arc::new(opa::Client::new(opa::Config::new(uri)).unwrap());
    let policy = opa::Policy::new(client, opa::Identity::new("alice")).unwrap();
    let mut connection =
        Connection::new(Session::default(), Schema::from_json("{}").unwrap()).with_opa(policy);
    for _ in 0..2 {
        let query = Query::cypher("RETURN $user").parameter("user", "admin");
        let result = connection.query(query).await;
        assert!(
            matches!(result,Err(orchiddb_client::Error::Query(ref message)) if message.contains("access denied"))
        );
    }
    assert_eq!(connection.into_session().executions, 0);
    server.join().unwrap();
}
