#![cfg(all(feature = "quickwit", feature = "elasticsearch"))]
use arrow::record_batch::RecordBatch;
use orchiddb::{
    federation::{self, Session},
    remote::transport::HttpSession,
};
use serde_json::{Value, json};
use std::collections::BTreeMap;

struct Local(duckdb::Connection);
#[async_trait::async_trait(?Send)]
impl Session for Local {
    fn dialect(&self) -> &str {
        "duckdb"
    }
    async fn query(&mut self, sql: &str) -> Result<Vec<RecordBatch>, String> {
        let mut statement = self.0.prepare(sql).map_err(|e| e.to_string())?;
        Ok(statement
            .query_arrow([])
            .map_err(|e| e.to_string())?
            .collect())
    }
}

#[tokio::test]
async fn live_remote_engines_compose_with_caller_owned_duckdb() {
    let Ok(path) = std::env::var("ORCHIDDB_REMOTE_FIXTURE") else {
        return;
    };
    let fixture: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        let db = duckdb::Connection::open_in_memory().unwrap();
        for sql in case["setup_sql"].as_array().unwrap() {
            db.execute_batch(sql.as_str().unwrap()).unwrap();
        }
        let remote = HttpSession::from_json_options(
            case["adapter"].as_str().unwrap(),
            json!({"endpoint":case["endpoint"],"page_size":2,"batch_size":2}),
        )
        .unwrap();
        let mut sessions: BTreeMap<String, Box<dyn Session>> = BTreeMap::from([
            ("local".into(), Box::new(Local(db)) as Box<dyn Session>),
            ("text".into(), Box::new(remote) as Box<dyn Session>),
        ]);
        let plan =
            orchiddb::compiler::compile(serde_json::from_value(case["request"].clone()).unwrap())
                .await
                .unwrap();
        for _ in 0..2 {
            let batches = federation::execute(&plan, &mut sessions).await.unwrap();
            let mut rows = vec![];
            for batch in batches {
                for row in 0..batch.num_rows() {
                    rows.push(
                        batch
                            .columns()
                            .iter()
                            .map(|a| {
                                orchiddb_client::arrow::util::display::array_value_to_string(a, row)
                                    .unwrap()
                            })
                            .collect::<Vec<_>>(),
                    );
                }
            }
            let expected: Vec<Vec<_>> = case["expected_rows"]
                .as_array()
                .unwrap()
                .iter()
                .map(|row| {
                    row.as_array()
                        .unwrap()
                        .iter()
                        .map(|v| {
                            v.as_str()
                                .map(str::to_owned)
                                .unwrap_or_else(|| v.to_string())
                        })
                        .collect()
                })
                .collect();
            assert_eq!(rows, expected, "{}", case["name"]);
        }
    }
}
