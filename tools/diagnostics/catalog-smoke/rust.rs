use orchiddb::catalog::{CatalogAuth, Credential, OrchidCatalog};
use orchiddb_client::{Connection, Query, SqlDialect, SqlSession, SqlWork};
use duckdb::arrow::{array::StringArray, datatypes::SchemaRef, error::ArrowError, record_batch::{RecordBatch, RecordBatchReader}};
struct Rows<'a>(duckdb::Arrow<'a>);
impl Iterator for Rows<'_> { type Item=Result<RecordBatch,ArrowError>; fn next(&mut self)->Option<Self::Item>{self.0.next().map(Ok)} }
impl RecordBatchReader for Rows<'_> { fn schema(&self)->SchemaRef{self.0.get_schema()} }
struct Session<'a>{db:&'a duckdb::Connection,statement:Option<duckdb::Statement<'a>>}
impl SqlSession for Session<'_> {
 type Error=duckdb::Error;
 type Output<'a>=Rows<'a> where Self:'a;
 fn dialect(&self)->SqlDialect{SqlDialect::DuckDb}
 async fn query<'a>(&'a mut self,work:&SqlWork)->Result<Rows<'a>,Self::Error>{self.statement=Some(self.db.prepare(&work.sql)?);self.statement.as_mut().unwrap().query_arrow([]).map(Rows)}
}
fn main(){tokio::runtime::Runtime::new().unwrap().block_on(async {
 let env=|key|std::env::var(key).unwrap();
 let auths=vec![CatalogAuth::bearer(Credential::value(env("AUTH_SMOKE_TOKEN"))),CatalogAuth::bearer(Credential::env("AUTH_SMOKE_TOKEN")),
 CatalogAuth::bearer(Credential::file(env("AUTH_SMOKE_TOKEN_FILE"))),CatalogAuth::client_credentials("admin",Credential::value(env("AUTH_SMOKE_SECRET"))),
 CatalogAuth::client_credentials("admin",Credential::env("AUTH_SMOKE_SECRET")),CatalogAuth::client_credentials("admin",Credential::file(env("AUTH_SMOKE_SECRET_FILE"))),
 CatalogAuth::client_credentials("external-client",Credential::value(env("AUTH_SMOKE_IDP_SECRET"))).with_issuer(env("AUTH_SMOKE_ISSUER")),CatalogAuth::token_exchange(Credential::value(env("AUTH_SMOKE_TOKEN")))];
 let db=duckdb::Connection::open_in_memory().unwrap();db.execute_batch("CREATE TABLE people(id BIGINT,name VARCHAR); INSERT INTO people VALUES (1,'Ada'),(2,'Grace')").unwrap();
 for auth in auths {
  let catalog=std::sync::Arc::new(OrchidCatalog::with_auth(&env("AUTH_SMOKE_URL"),auth,"smoke","smoke").unwrap());
  assert_eq!(catalog.discover(None).await.unwrap()["description"],"Auth smoke graph");
  let mut graph=Connection::with_catalog(Session{db:&db,statement:None},catalog);
  let rows=graph.query(Query::cypher("MATCH (p:Person)-[:PEER {score:1, limit:1}]->(q:Person) RETURN q.name AS name ORDER BY name")).await.unwrap();
  let mut names=Vec::new();for batch in rows {let batch=batch.unwrap();let column=batch.column(0).as_any().downcast_ref::<StringArray>().unwrap();names.extend(column.iter().map(|v|v.unwrap().to_owned()));}assert_eq!(names,vec!["Ada","Grace"]);
 }
 println!("PASS Rust: credential sources, OAuth/OIDC, exchange, DuckDB execution");
});}
