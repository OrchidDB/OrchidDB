#![cfg(feature = "duckdb")]
use orchiddb::engine::GraphEngine;
use serde_json::Value;

fn native(result: &orchiddb::engine::QueryResult, language: &str) -> Value {
    let key = format!("orchiddb.{language}.typed_rows.v1");
    let schema = result.returned.batch.schema();
    serde_json::from_str(schema.metadata().get(&key).expect("typed result boundary")).unwrap()
}

#[tokio::test]
async fn sql_islands_preserve_native_graph_results() {
    let mut engine = GraphEngine::in_memory().unwrap();
    engine.cypher("CREATE (:Person {name:'a'}), (:Person {name:'b'})").await.unwrap();
    let cypher = engine.cypher("MATCH (n:Person) RETURN n").await.unwrap();
    let rows = native(&cypher, "cypher");
    assert_eq!(rows.as_array().unwrap().len(), 2);
    assert_eq!(rows[0][0]["type"], "vertex");
    assert!(cypher.stats.islands > 0, "graph scans still use SQL islands");
    let gremlin = engine.gremlin("g.V().has('name','a')").await.unwrap();
    let rows = native(&gremlin, "gremlin");
    assert_eq!(rows.as_array().unwrap().len(), 1);
    assert_eq!(rows[0][0]["type"], "vertex");
    assert!(gremlin.stats.islands > 0, "filtered scan still uses SQL islands");
}

#[tokio::test]
async fn sql_islands_do_not_reinterpret_literal_graph_text() {
    let mut engine = GraphEngine::in_memory().unwrap();
    let result = engine.cypher("WITH 'v[not-a-vertex]' AS s RETURN s").await.unwrap();
    let rows = native(&result, "cypher");
    assert_eq!(rows[0][0]["type"], "string");
    assert_eq!(rows[0][0]["value"], "v[not-a-vertex]");
}

#[tokio::test]
async fn gremlin_set_operations_preserve_sets_and_reject_non_iterables() {
    let mut engine = GraphEngine::in_memory().unwrap();
    for (step, expected) in [("difference", 1), ("intersect", 1), ("disjunct", 2), ("merge", 3)] {
        let query = format!("g.inject([1,1,2]).{step}([2,3])");
        let result = engine.gremlin(&query).await.unwrap();
        let rows = native(&result, "gremlin");
        assert_eq!(rows[0][0]["type"], "set", "{query}: {rows}");
        assert_eq!(rows[0][0]["value"].as_array().unwrap().len(), expected, "{query}");
    }
    let error = engine.gremlin("g.inject(1).difference([2])").await.err().unwrap();
    assert!(error.contains("Iterable type for incoming traversers"), "{error}");
    let error = engine.gremlin("g.inject([1]).difference(null)").await.err().unwrap();
    assert!(error.contains("Argument provided for difference step can't be null"), "{error}");
}

#[tokio::test]
async fn gremlin_maps_stay_native_across_sql_boundaries() {
    let mut engine = GraphEngine::in_memory().unwrap();
    engine.cypher("CREATE (:Person {name:'a'}), (:Person {name:'b'})").await.unwrap();
    for query in ["g.V().valueMap('name')", "g.V().values('name').groupCount()", "g.V().project('n').by('name')"] {
        let result = engine.gremlin(query).await.unwrap();
        let rows = native(&result, "gremlin");
        assert!(!rows.as_array().unwrap().is_empty(), "{query}");
        for row in rows.as_array().unwrap() {
            assert_eq!(row[0]["type"], "map", "{query}: {rows}");
        }
    }
}

#[tokio::test]
async fn sql_preserves_composed_null_predicate_precedence() {
    let query = "UNWIND [true,false,null] AS a UNWIND [true,false,null] AS b UNWIND [true,false,null] AS c RETURN (a OR (b AND c)) IS NULL = ((a OR b) AND (a OR c)) IS NULL AS result";
    let graph = orchiddb::ir::catalog::PropertyGraph::new();
    let parsed = orchiddb::language::cypher::parser::parse_query(query).unwrap();
    let plan = orchiddb::language::cypher::planner::CypherPlanner::new().plan(&parsed).unwrap();
    let lowered = orchiddb::ir::rel::RelBackend::new().lower(&plan, &graph).unwrap();
    let sql = orchiddb::ir::rel::sql::unparse(&lowered, orchiddb::ir::rel::sql::SqlDialect::DuckDb).unwrap();
    let mut engine = GraphEngine::in_memory().unwrap();
    let result = engine.cypher(query).await.unwrap();
    let rows = native(&result, "cypher");
    assert!(rows.as_array().unwrap().iter().all(|r| r[0]["value"] == true), "{sql}\n{rows}");
    let result = engine.cypher("UNWIND [true,false,null] AS a RETURN (NOT a) IS NULL = a IS NULL AS result").await.unwrap();
    let rows = native(&result, "cypher");
    assert!(rows.as_array().unwrap().iter().all(|r| r[0]["value"] == true), "{rows}");
}

#[tokio::test]
async fn gremlin_map_keys_preserve_types_and_do_not_collide_with_strings() {
    let mut engine = GraphEngine::in_memory().unwrap();
    for query in ["g.inject(1,1,'d[1].i').groupCount()", "g.inject(1,1,'d[1].i').groupCount('counts').cap('counts')"] {
        let result = engine.gremlin(query).await.unwrap();
        let rows = native(&result, "gremlin");
        let entries = rows[0][0]["value"].as_array().unwrap();
        assert_eq!(entries.len(), 2, "{query}: {rows}");
        assert!(entries.iter().any(|e| e[0]["type"] == "int" && e[1]["value"] == 2), "{rows}");
        assert!(entries.iter().any(|e| e[0]["type"] == "string" && e[1]["value"] == 1), "{rows}");
    }
    engine.cypher("CREATE (:Person {name:'a', `t[id]`:'literal'})").await.unwrap();
    let result = engine.gremlin("g.V().elementMap()").await.unwrap();
    let rows = native(&result, "gremlin");
    let entries = rows[0][0]["value"].as_array().unwrap();
    assert!(entries.iter().any(|e| e[0]["type"] == "token" && e[0]["value"] == "id"), "{rows}");
    assert!(entries.iter().any(|e| e[0]["type"] == "string" && e[0]["value"] == "t[id]"), "{rows}");
}

#[tokio::test]
async fn gremlin_repeated_paths_keep_every_graph_object() {
    let mut engine = GraphEngine::in_memory().unwrap();
    engine.cypher("CREATE (a:Person {name:'a'})-[:knows]->(b:Person {name:'b'})-[:knows]->(c:Person {name:'c'})").await.unwrap();
    let result = engine.gremlin("g.V().has('name','a').repeat(__.out()).times(2).emit().path()").await.unwrap();
    let rows = native(&result, "gremlin");
    assert_eq!(rows.as_array().unwrap().len(), 2, "{rows}");
    let mut lengths = Vec::new();
    for row in rows.as_array().unwrap() {
        assert_eq!(row[0]["type"], "path", "{rows}");
        let path = row[0]["value"].as_array().unwrap();
        assert!(path.iter().all(|item| item["type"] == "vertex"), "{rows}");
        lengths.push(path.len());
    }
    lengths.sort();
    assert_eq!(lengths, vec![2,3]);
}

#[tokio::test]
async fn gremlin_fold_and_cap_preserve_collection_semantics() {
    let mut engine = GraphEngine::in_memory().unwrap();
    let result = engine.gremlin("g.V().fold()").await.unwrap();
    let rows = native(&result, "gremlin");
    assert_eq!(rows[0][0]["type"], "list");
    assert_eq!(rows[0][0]["value"], serde_json::json!([]));
    let result = engine.gremlin("g.inject(null,1).fold()").await.unwrap();
    let rows = native(&result, "gremlin");
    assert_eq!(rows[0][0]["value"].as_array().unwrap().len(), 2, "{rows}");
    let result = engine.gremlin("g.inject(1,1,2).aggregate('x').cap('x')").await.unwrap();
    let rows = native(&result, "gremlin");
    assert_eq!(rows.as_array().unwrap().len(), 1, "{rows}");
    assert_eq!(rows[0][0]["type"], "bulkset");
    assert_eq!(rows[0][0]["value"].as_array().unwrap().len(), 3, "{rows}");
    for suffix in ["max(local)", "unfold().max()"] {
        let result = engine.gremlin(&format!("g.inject(1,1,2).aggregate('x').cap('x').{suffix}")).await.unwrap();
        assert_eq!(native(&result, "gremlin")[0][0]["value"], 2);
    }
    let result = engine.gremlin("g.V().values('missing').max()").await.unwrap();
    assert_eq!(native(&result, "gremlin"), serde_json::json!([]));
}
