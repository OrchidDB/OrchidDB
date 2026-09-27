#![cfg(feature = "duckdb")]
use orchiddb::engine::GraphEngine;

async fn songs() -> GraphEngine {
    let mut e = GraphEngine::in_memory().unwrap();
    e.cypher("CREATE (g:artist {name:'Garcia'}), (w:artist {name:'Willie_Dixon'}), (a:song {name:'a',songType:'ballad'}), (b:song {name:'b',songType:'ballad'}), (c:song {name:'c',songType:'rock'}), (a)-[:sungBy]->(g), (b)-[:sungBy]->(g), (a)-[:writtenBy]->(w), (c)-[:writtenBy]->(w), (a)-[:followedBy {weight:2}]->(b), (b)-[:followedBy {weight:3}]->(c)").await.unwrap();
    e
}
fn cells(result: &orchiddb::engine::QueryResult) -> Vec<String> {
    (0..result.returned.batch.num_rows()).map(|row|
        arrow::util::display::array_value_to_string(result.returned.batch.column(0), row).unwrap()).collect()
}
#[tokio::test]
async fn labelled_source_join_pushes_selective_predicates_into_sql() {
    let mut e = songs().await;
    let r = e.gremlin("g.V().has('artist','name','Garcia').in('sungBy').as('song').V().has('artist','name','Willie_Dixon').in('writtenBy').where(P.eq('song')).values('name')").await.unwrap();
    assert_eq!(cells(&r), vec!["a"]);
    assert!(r.stats.sql_queries.iter().any(|sql| sql.contains("Garcia") && sql.contains("Willie_Dixon")), "{:?}", r.stats);
    assert!(r.stats.cost.native_kernel_calls < 5, "{:?}", r.stats);
}
#[tokio::test]
async fn match_preserves_element_identity_and_duplicate_matches() {
    let mut e = songs().await;
    let r = e.gremlin("g.V().match(__.as('a').out('followedBy').as('b'),__.as('b').in('followedBy').as('a')).select('a').by('name')").await.unwrap();
    let mut got = cells(&r); got.sort(); assert_eq!(got, vec!["a", "b"]);
    assert!(r.stats.sql_queries.iter().any(|sql| sql.contains("JOIN")), "{:?}", r.stats);
}
#[tokio::test]
async fn nested_group_uses_scalar_key_projection() {
    let mut e = songs().await;
    let r = e.gremlin("g.V().out('followedBy').group().by('songType').by(__.bothE('followedBy').group().by(T.label).by(__.values('weight').sum()))").await.unwrap();
    let value = cells(&r).join("");
    assert!(value.contains("ballad") && value.contains("rock") && value.contains("5") && value.contains("3"), "{value}");
    assert!(!r.stats.physical_plan.contains("LateralApply"), "{}", r.stats.physical_plan);
    assert!(r.stats.cost.native_kernel_calls < 25, "{:?}", r.stats);
}
#[tokio::test]
async fn path_and_label_history_observers_keep_real_traverser_values() {
    let mut e = songs().await;
    let path = e.gremlin("g.V().has('name','a').out('followedBy').path().by('name')").await.unwrap();
    let text = cells(&path).join(""); assert!(text.contains("a") && text.contains("b"), "{text}");
    let history = e.gremlin("g.V().has('name','a').as('x').out('followedBy').as('x').select(Pop.first,'x').values('name')").await.unwrap();
    assert_eq!(cells(&history), vec!["a"]);
}
#[tokio::test]
async fn match_repeated_scalar_labels_are_equality_constraints() {
    let mut e = songs().await;
    let r = e.gremlin("g.V().match(__.as('a').out('followedBy').count().as('b'),__.as('a').in('followedBy').count().as('b')).count()").await.unwrap();
    assert_eq!(cells(&r), vec!["3"]);
    assert!(!r.stats.physical_plan.contains("LateralApply"), "{}", r.stats.physical_plan);
}
#[tokio::test]
async fn grouping_preserves_duplicate_members_and_empty_child_groups() {
    let mut e = songs().await;
    e.cypher("MATCH (a:song {name:'a'}),(b:song {name:'b'}) CREATE (a)-[:followedBy {weight:7}]->(b)").await.unwrap();
    let r = e.gremlin("g.V().out('followedBy').group().by('songType').by(__.bothE('followedBy').group().by(T.label).by(__.values('weight').sum()))").await.unwrap();
    let value = cells(&r).join("");
    assert!(value.contains("24") && value.contains("3"), "{value}");
    assert!(r.stats.sql_queries.iter().any(|sql| sql.contains("GROUP BY")), "{:?}", r.stats);
    let empty = e.gremlin("g.V().hasLabel('song').group().by('songType').by(__.outE('absent').group().by(T.label).by(__.values('weight').sum()))").await.unwrap();
    let value = cells(&empty).join("");
    assert!(value.contains("ballad") && value.contains("rock"), "{value}");
}
#[tokio::test]
async fn productive_null_labels_remain_distinct_from_absent_labels() {
    let mut e = GraphEngine::in_memory().unwrap();
    let result = e.gremlin("g.inject(null).as('x').select('x')").await.unwrap();
    assert_eq!(result.returned.batch.num_rows(), 1);
    let first = e.gremlin("g.inject(1).as('x').constant(2).as('x').select(Pop.first,'x')").await.unwrap();
    assert_eq!(cells(&first), vec!["1"]);
}
#[tokio::test]
async fn anchored_where_filters_run_before_match_map_assembly() {
    let mut e = songs().await;
    let r = e.gremlin("g.V().match(__.as('a').out('followedBy').as('b')).where(__.as('b').has('name','c'))").await.unwrap();
    assert_eq!(r.returned.batch.num_rows(), 1);
    assert_eq!(r.stats.cost.sql_output_rows, 1, "{:?}", r.stats);
    assert!(!r.stats.physical_plan.contains("LateralApply"), "{}", r.stats.physical_plan);
}
#[tokio::test]
async fn scalar_match_bindings_select_count_remains_relational() {
    let mut e = songs().await;
    let r = e.gremlin("g.V().hasLabel('song').match(__.as('a').values('name').as('b'),__.as('a').values('songType').as('c')).select('b','c').count()").await.unwrap();
    assert_eq!(cells(&r), vec!["3"]);
    assert_eq!(r.stats.cost.sql_output_rows, 1, "{:?}", r.stats);
    assert!(!r.stats.physical_plan.contains("LateralApply"), "{}", r.stats.physical_plan);
}
#[tokio::test]
async fn scalar_history_pop_keeps_list_shape_and_first_entry() {
    let mut e = GraphEngine::in_memory().unwrap();
    let first = e.gremlin("g.inject(1).as('x').constant(2).as('x').select(Pop.first,'x')").await.unwrap();
    assert_eq!(cells(&first), vec!["1"]);
    let all = e.gremlin("g.inject(1).as('x').select(Pop.all,'x')").await.unwrap();
    assert!(cells(&all)[0].contains('['), "{:?}", cells(&all));
    let last = e.gremlin("g.inject(1).as('x').constant(2).as('x').select(Pop.last,'x')").await.unwrap();
    assert_eq!(cells(&last), vec!["2"]);
}
#[tokio::test]
async fn anchored_where_correlates_label_bindings_per_occurrence() {
    let mut e = songs().await;
    e.cypher("MATCH (a:song {name:'a'}),(c:song {name:'c'}) CREATE (a)-[:followedBy]->(c)").await.unwrap();
    let r = e.gremlin("g.V().match(__.as('a').out('followedBy').as('b')).where(__.as('b').has('name','c'))").await.unwrap();
    assert_eq!(r.returned.batch.num_rows(), 2, "{:?}", cells(&r));
    assert_eq!(r.stats.cost.sql_output_rows, 2, "{:?}", r.stats);
}
