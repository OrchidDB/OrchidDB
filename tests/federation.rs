use arrow::record_batch::RecordBatch;
use orchiddb::{
    compiler::{CompileRequest, compile},
    federation::{self, Session},
};
use serde_json::{Value, json};
use std::{cell::RefCell, collections::BTreeMap, rc::Rc};

fn request() -> Value {
    json!({"version":1,"dialect":"duckdb","language":"cypher",
    "query":"MATCH (a:Person) WHERE a.age > 25 RETURN count(a) AS n",
    "engines":{"local":{"dialect":"duckdb"},"remote":{"dialect":"postgres"}},"execution_engine":"local",
    "tables":[{"name":"people","engine":"remote","columns":[{"name":"id","data_type":"int64"},{"name":"age","data_type":"int64"},{"name":"unused","data_type":"string"}]}],
    "nodes":[{"label":"Person","table":"people","id":"id","properties":{"age":"age"}}]})
}
async fn plan(r: Value) -> Result<orchiddb::compiler::CompiledSql, String> {
    compile(serde_json::from_value::<CompileRequest>(r).unwrap()).await
}

#[tokio::test]
async fn maximal_island_pushes_filter_projection_and_aggregate() {
    let p = plan(request()).await.unwrap();
    assert_eq!(p.transfers.len(), 1);
    let t = &p.transfers[0];
    assert_eq!(t.source_engine, "remote");
    assert_eq!(t.source_dialect, "postgres");
    assert!(t.sql.to_lowercase().contains("count("), "{}", t.sql);
    assert!(t.sql.contains("25"));
    assert!(!t.sql.contains("unused"));
    assert!(!p.sql.contains("people"));
    assert!(p.sql.contains(&t.target_relation));
}
#[tokio::test]
async fn engine_validation_and_local_execution() {
    let mut r = request();
    r["execution_engine"] = json!("missing");
    assert!(
        plan(r)
            .await
            .unwrap_err()
            .contains("unknown execution_engine")
    );
    let mut r = request();
    r["tables"][0]["engine"] = json!("missing");
    assert!(plan(r).await.unwrap_err().contains("unknown engine"));
    let mut r = request();
    r["engines"]["local"]["dialect"] = json!("postgres");
    assert!(plan(r).await.unwrap_err().contains("dialect"));
    let mut r = request();
    r["tables"][0]["engine"] = json!("local");
    assert!(plan(r).await.unwrap().transfers.is_empty());
    let mut r = request();
    r["query"] = json!("RETURN 1 AS n");
    assert!(plan(r).await.unwrap().transfers.is_empty());
}
struct Mock {
    dialect: &'static str,
    log: Rc<RefCell<Vec<String>>>,
    fail: bool,
}
#[async_trait::async_trait(?Send)]
impl Session for Mock {
    fn dialect(&self) -> &str {
        self.dialect
    }
    async fn query(&mut self, sql: &str) -> Result<Vec<RecordBatch>, String> {
        self.log
            .borrow_mut()
            .push(format!("query:{}", self.dialect));
        if self.fail {
            Err("query failure".into())
        } else {
            assert!(!sql.to_ascii_uppercase().contains("CREATE TABLE"));
            Ok(vec![])
        }
    }
}
#[tokio::test]
async fn routing_preflight_and_cleanup_after_final_error() {
    let p = plan(request()).await.unwrap();
    let log = Rc::new(RefCell::new(vec![]));
    let mut sessions: BTreeMap<String, Box<dyn Session>> = BTreeMap::new();
    sessions.insert(
        "local".into(),
        Box::new(Mock {
            dialect: "duckdb",
            log: log.clone(),
            fail: true,
        }),
    );
    assert!(
        federation::execute(&p, &mut sessions)
            .await
            .unwrap_err()
            .contains("missing engine")
    );
    assert!(log.borrow().is_empty());
    sessions.insert(
        "remote".into(),
        Box::new(Mock {
            dialect: "postgres",
            log: log.clone(),
            fail: false,
        }),
    );
    assert_eq!(
        federation::execute(&p, &mut sessions).await.unwrap_err(),
        "query failure"
    );
    assert_eq!(log.borrow().len(), 2);
    assert_eq!(log.borrow().last().unwrap(), "query:duckdb");
}

#[tokio::test]
async fn unsupported_exchange_cannot_read_same_named_local_table() {
    let mut r = request();
    // Structs are now transferable; duration still has no exchange codec.
    r["tables"][0]["columns"][1]["data_type"] = json!("duration");
    r["query"] = json!("MATCH (a:Person) RETURN a.age");
    assert!(
        plan(r)
            .await
            .unwrap_err()
            .contains("cannot transfer source")
    );
    let mut r = request();
    let mut duplicate = r["tables"][0].clone();
    duplicate["name"] = json!("\"people\"");
    r["tables"].as_array_mut().unwrap().push(duplicate);
    assert!(
        plan(r)
            .await
            .unwrap_err()
            .contains("duplicate SQL table identity")
    );
}

#[tokio::test]
async fn binding_is_a_query_scoped_cte_with_typed_empty_rows() {
    for dialect in ["duckdb", "postgres"] {
        let mut r = request();
        r["dialect"] = json!(dialect);
        r["engines"]["local"]["dialect"] = json!(dialect);
        let p = plan(r).await.unwrap();
        let command =
            json!({"op":"bind", "plan":p, "relation":p.transfers[0].target_relation, "rows":[]});
        let bound = federation::bind_command(command).unwrap();
        assert!(bound["transfers"].as_array().unwrap().is_empty());
        let sql = bound["sql"].as_str().unwrap();
        assert!(sql.starts_with("WITH "));
        assert!(sql.contains("CAST(NULL AS BIGINT)"));
        assert!(sql.contains("WHERE false") || sql.contains("WHERE FALSE"));
        assert!(!sql.contains("CREATE "));
    }
}

#[tokio::test]
async fn binding_rejects_missing_relations_and_wrong_row_shapes() {
    let p = plan(request()).await.unwrap();
    let mut command = json!({"op":"bind", "plan":p, "relation":"missing", "rows":[[1]]});
    assert!(
        federation::bind_command(command.clone())
            .unwrap_err()
            .contains("unknown exchange")
    );
    command["relation"] = json!(p.transfers[0].target_relation);
    command["rows"] = json!([[1, 2]]);
    assert!(
        federation::bind_command(command)
            .unwrap_err()
            .contains("width mismatch")
    );
}
