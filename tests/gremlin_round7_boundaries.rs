#![cfg(feature = "duckdb")]
use orchiddb::engine::GraphEngine;
use serde_json::Value;
fn native(result: &orchiddb::engine::QueryResult) -> Value {
    serde_json::from_str(result.returned.batch.schema().metadata().get("crabgraph.gremlin.typed_rows.v1").unwrap()).unwrap()
}
#[tokio::test]
async fn has_id_matches_public_identity_after_expansion() {
    let mut engine=GraphEngine::in_memory().unwrap();
    engine.cypher("CREATE (:person {name:'a'})-[:knows]->(:person {name:'b'})").await.unwrap();
    for query in ["g.V('person#0').out().hasId('person#1')", "g.V('person#0').out().hasId(P.neq('person#0'))"] {
        let result=engine.gremlin(query).await.unwrap();
        assert_eq!(native(&result).as_array().unwrap().len(),1,"{query}");
    }
}
#[tokio::test]
async fn heterogeneous_numeric_where_preserves_numeric_equality() {
    let mut engine=GraphEngine::in_memory().unwrap();
    let result=engine.gremlin("g.inject([1B,1S,1I,1L,1.0F,1.0D,1000I,1D,1N]).unfold().where(__.is(1B))").await.unwrap();
    assert_eq!(native(&result).as_array().unwrap().len(),8,"{}",native(&result));
}

#[tokio::test]
async fn nan_predicate_equality_is_not_total_order_equality() {
    let mut engine=GraphEngine::in_memory().unwrap();
    for (query,count) in [("g.inject(NaN).is(P.eq(NaN))",0),("g.inject(NaN).is(P.neq(NaN))",1)] {
        assert_eq!(native(&engine.gremlin(query).await.unwrap()).as_array().unwrap().len(),count);
    }
}

#[tokio::test]
async fn has_traversal_tests_property_or_token_and_keeps_element() {
    let mut engine=GraphEngine::in_memory().unwrap();
    engine.cypher("CREATE (:person {age:29}), (:person {age:35}), (:software {name:'app'})").await.unwrap();
    for (query,count) in [("g.V().has('age',__.is(P.gt(30)))",1),("g.V().has(T.label,__.is('person'))",2),("g.V().has('person','age',__.is(P.gt(30)))",1)] {
        let rows=native(&engine.gremlin(query).await.unwrap());
        assert_eq!(rows.as_array().unwrap().len(),count,"{query}: {rows}");
        assert_eq!(rows[0][0]["type"],"vertex");
    }
}
