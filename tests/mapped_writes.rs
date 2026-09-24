#![cfg(feature="duckdb")]
use std::sync::Arc;
use arrow::datatypes::{Schema,Field,DataType};
use new_graph::{mapped_engine::MappedGraphEngine,ir::rel::{mapping::{GraphMapping,NodeMapping,EdgeMapping},sql::DuckDbExecutor}};
fn engine()->MappedGraphEngine {
 let mut m=GraphMapping::new();
 for (table,cols) in [("people",vec![("person_id",DataType::Int64),("full_name",DataType::Utf8),("score",DataType::Int64)]),("teams",vec![("team_id",DataType::Int64),("title",DataType::Utf8)]),("memberships",vec![("membership_id",DataType::Int64),("person_fk",DataType::Int64),("team_fk",DataType::Int64),("strength",DataType::Int64)])]{
  m.register_table_schema(table,Arc::new(Schema::new(cols.into_iter().map(|(n,t)|Field::new(n,t,true)).collect::<Vec<_>>())));
 }
 m.map_node(NodeMapping::table("Person","people","person_id").property("identity","person_id").property("name","full_name").property("score","score"));
 m.map_node(NodeMapping::table("Team","teams","team_id").property("name","title"));
 m.map_edge(EdgeMapping::table("MEMBER","memberships","person_fk","team_fk","Person","Team").with_id("membership_id").property("weight","strength"));
 let mut e=MappedGraphEngine::new(DuckDbExecutor::new(),Arc::new(m));
 e.execute_sql("CREATE TABLE people(person_id BIGINT PRIMARY KEY,full_name VARCHAR NOT NULL,score BIGINT DEFAULT 10); CREATE TABLE teams(team_id BIGINT PRIMARY KEY,title VARCHAR NOT NULL); CREATE TABLE memberships(membership_id BIGINT PRIMARY KEY,person_fk BIGINT REFERENCES people(person_id),team_fk BIGINT REFERENCES teams(team_id),strength BIGINT)").unwrap();e
}
fn scalar(e:&mut MappedGraphEngine,sql:&str)->i64{e.executor_mut().connection().unwrap().query_row(sql,[],|r|r.get(0)).unwrap()}
#[tokio::test]async fn cypher_writes_resolve_all_three_tables_and_preserve_read_mapping(){
 let mut e=engine();
 e.cypher("CREATE (p:Person {identity:42,name:'Alice',score:7}), (t:Team {name:'Engineering'}), (p)-[:MEMBER {weight:3}]->(t) RETURN p.name,t.name").await.unwrap();
 assert_eq!(scalar(&mut e,"SELECT count(*) FROM people WHERE person_id=42 AND full_name='Alice'"),1);
 assert_eq!(scalar(&mut e,"SELECT count(*) FROM memberships WHERE person_fk=42 AND team_fk=1 AND strength=3"),1);
 let r=e.gremlin("g.V().hasLabel('Person').out('MEMBER').values('name')").await.unwrap();assert_eq!(r.batch.num_rows(),1);
 e.cypher("MATCH (p:Person)-[r:MEMBER]->(t:Team) SET p.score=p.score+2, t.name='Platform', r.weight=8 RETURN p.score").await.unwrap();
 assert_eq!(scalar(&mut e,"SELECT score FROM people"),9);assert_eq!(scalar(&mut e,"SELECT strength FROM memberships"),8);
 assert!(e.cypher("MATCH (p:Person) DELETE p").await.is_err());
 assert_eq!(scalar(&mut e,"SELECT count(*) FROM people"),1);
 e.cypher("MATCH ()-[r:MEMBER]->() DELETE r").await.unwrap();
 e.cypher("MATCH (p:Person) DELETE p").await.unwrap();
 assert_eq!(scalar(&mut e,"SELECT count(*) FROM people"),0);assert_eq!(scalar(&mut e,"SELECT count(*) FROM memberships"),0);assert_eq!(scalar(&mut e,"SELECT count(*) FROM teams"),1);
}
#[tokio::test]async fn mapped_writes_rollback_all_tables_on_failure_and_join_transactions(){
 let mut e=engine();
 assert!(e.cypher("CREATE (:Person {name:'temporary'}), (:Team {name:null})").await.is_err());
 assert_eq!(scalar(&mut e,"SELECT count(*) FROM people"),0);
 e.executor_mut().begin().unwrap();e.cypher("CREATE (:Person {name:'temporary'})").await.unwrap();
 assert_eq!(scalar(&mut e,"SELECT count(*) FROM people"),1);e.executor_mut().rollback().unwrap();
 assert_eq!(scalar(&mut e,"SELECT count(*) FROM people"),0);
}
#[tokio::test]async fn gremlin_inserts_updates_and_deletes_use_mapped_rows(){
 let mut e=engine();
 e.gremlin("g.addV('Person').property('name','Bob').property('score',20)").await.unwrap();
 assert_eq!(scalar(&mut e,"SELECT count(*) FROM people WHERE full_name='Bob' AND score=20"),1);
 e.gremlin("g.V().hasLabel('Person').has('name','Bob').property('score',21)").await.unwrap();
 assert_eq!(scalar(&mut e,"SELECT score FROM people"),21);
 e.gremlin("g.V().hasLabel('Person').drop()").await.unwrap();
 assert_eq!(scalar(&mut e,"SELECT count(*) FROM people"),0);
}
