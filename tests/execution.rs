use orchiddb::{
    compiler::CompiledSql,
    execution::{ExecutionError, SqlSession, execute},
    ir::rel::sql::SqlDialect,
};

struct Session {
    calls: usize,
    sql: String,
    fail: bool,
}
impl SqlSession for Session {
    type Error = &'static str;
    type Output<'a> = &'a str;
    fn dialect(&self) -> SqlDialect {
        SqlDialect::DuckDb
    }
    async fn query<'a>(&'a mut self, query: &CompiledSql) -> Result<&'a str, Self::Error> {
        self.calls += 1;
        if self.fail {
            return Err("driver failure");
        }
        self.sql = query.sql.clone();
        Ok(&self.sql)
    }
}
fn query() -> CompiledSql {
    CompiledSql {
        version: 1,
        dialect: "duckdb".into(),
        sql: "SELECT 42".into(),
        fields: vec!["answer".into()],
    }
}
#[tokio::test]
async fn lends_results_and_preserves_the_callers_session() {
    let mut session = Session {
        calls: 0,
        sql: String::new(),
        fail: false,
    };
    assert_eq!(execute(&mut session, &query()).await.unwrap(), "SELECT 42");
    assert_eq!(session.calls, 1);
    assert_eq!(execute(&mut session, &query()).await.unwrap(), "SELECT 42");
    assert_eq!(session.calls, 2);
}
#[tokio::test]
async fn rejects_incompatible_plans_before_driver_execution() {
    let mut session = Session {
        calls: 0,
        sql: String::new(),
        fail: false,
    };
    let mut q = query();
    q.version = 2;
    assert!(matches!(
        execute(&mut session, &q).await,
        Err(ExecutionError::Version(2))
    ));
    q.version = 1;
    q.dialect = "postgres".into();
    assert!(matches!(
        execute(&mut session, &q).await,
        Err(ExecutionError::Dialect { .. })
    ));
    assert_eq!(session.calls, 0);
    session.fail = true;
    assert!(matches!(
        execute(&mut session, &query()).await,
        Err(ExecutionError::Driver("driver failure"))
    ));
    assert_eq!(session.calls, 1);
}
