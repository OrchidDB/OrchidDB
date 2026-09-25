#![cfg(feature = "duckdb")]
use orchiddb::engine::GraphEngine;
use serde_json::Value;
fn native(result: &orchiddb::engine::QueryResult) -> Value {
    serde_json::from_str(result.returned.batch.schema().metadata().get("orchiddb.gremlin.typed_rows.v1").unwrap()).unwrap()
}
#[tokio::test]
async fn decimal_and_double_remain_distinct_in_numeric_equality_results() {
    let mut engine=GraphEngine::in_memory().unwrap();
    let rows=native(&engine.gremlin("g.inject([1B,1S,1I,1L,1.0F,1.0D,1000I,1M,1N]).unfold().where(__.is(1B))").await.unwrap());
    let types=rows.as_array().unwrap().iter().map(|r|r[0]["type"].as_str().unwrap()).collect::<Vec<_>>();
    assert_eq!(types,vec!["byte","short","int","long","float","double","bigdecimal","bigint"]);
}
#[tokio::test]
async fn map_entries_have_native_identity_and_scalar_column_projections() {
    let mut engine=GraphEngine::in_memory().unwrap();
    let rows=native(&engine.gremlin("g.inject('b','a','b').groupCount().unfold().order().by(Column.values,Order.desc)").await.unwrap());
    assert_eq!(rows[0][0]["type"],"entry");
    assert_eq!(rows[0][0]["value"][0]["value"],"b");
    assert_eq!(rows[0][0]["value"][1]["value"],2);
    let rows=native(&engine.gremlin("g.inject('a','a').groupCount().unfold().select(Column.values)").await.unwrap());
    assert_eq!(rows[0][0]["value"],2);
    let rows=native(&engine.gremlin("g.inject('key','value').groupCount()").await.unwrap());
    assert_eq!(rows[0][0]["type"],"map");
}
#[tokio::test]
async fn both_expansion_keeps_properties_for_sort_and_projection() {
    let mut engine=GraphEngine::in_memory().unwrap();
    engine.cypher("CREATE (a:person {name:'a',age:29})-[:knows]->(:person {name:'b',age:35}), (a)-[:created]->(:software {name:'c'})").await.unwrap();
    let rows=native(&engine.gremlin("g.V().both().hasLabel('person').order().by('age',Order.desc).values('name')").await.unwrap());
    let names=rows.as_array().unwrap().iter().map(|r|r[0]["value"].as_str().unwrap()).collect::<Vec<_>>();
    assert_eq!(names,vec!["b","a","a"]);
}

#[tokio::test]
async fn undefined_ordered_nan_comparison_survives_negation() {
    let mut engine=GraphEngine::in_memory().unwrap();
    for (query,count) in [
        ("g.inject(1.0D).not(__.is(P.gt(NaN)))",0),
        ("g.inject(1.0D).not(__.is(P.eq(NaN)))",1),
        ("g.inject(1.0D).is(P.lt(NaN).or(P.gt(0)))",1),
        ("g.inject(1.0D).not(__.is(P.lt(NaN).and(P.gt(2))))",1),
    ] {
        assert_eq!(native(&engine.gremlin(query).await.unwrap()).as_array().unwrap().len(),count,"{query}");
    }
}
