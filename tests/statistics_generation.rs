use orchiddb::{
    compiler::compile_json,
    ir::rel::statistics::{self, Generator},
};
use serde_json::{Value, json};
fn request() -> Value {
    json!({"version":1,"dialect":"duckdb","language":"cypher","query":"MATCH (n:N) WHERE n.name='rare' RETURN n.name","tables":[{"name":"people","columns":[{"name":"id","data_type":"int64"},{"name":"name","data_type":"string"}]}],"nodes":[{"label":"N","table":"people","id":"id","properties":{"name":"name"}}]})
}
async fn cmd(v: Value) -> Value {
    serde_json::from_str(&statistics::command(&v.to_string()).await.unwrap()).unwrap()
}
#[tokio::test]
async fn protocol_cached_compile_all_languages_and_release() {
    let r = request();
    let started = cmd(json!({"op":"begin","request":r})).await;
    let id = started["id"].clone();
    let mut next = started;
    while !next["request"].is_null() {
        let task = &next["request"];
        let rows = if task["kind"] == "metadata" {
            json!([{"rows":4,"bytes":128}])
        } else {
            json!([{"id":1,"name":"rare"},{"id":2,"name":"common"},{"id":3,"name":"common"},{"id":4,"name":"common"}])
        };
        next = cmd(json!({"op":"submit","id":id,"request_id":task["id"],"rows":rows})).await;
    }
    let finished = cmd(json!({"op":"finish","id":id})).await;
    let snapshot = &finished["snapshot"];
    assert_eq!(snapshot["sources"]["people"]["sample_rows"], 4);
    assert_eq!(
        snapshot["sources"]["people"]["columns"]["name"]["sample_distinct"],
        2
    );
    for (language, query) in [
        ("cypher", "MATCH (n:N) WHERE n.name='rare' RETURN n.name"),
        (
            "gremlin",
            "g.V().hasLabel('N').has('name','rare').values('name')",
        ),
    ] {
        let mut q = r.clone();
        q["language"] = json!(language);
        q["query"] = json!(query);
        let out =
            cmd(json!({"op":"compile","catalog_id":finished["catalog_id"],"request":q})).await;
        assert!(
            !out["plan_estimates"].as_array().unwrap().is_empty(),
            "{out}"
        );
        assert_eq!(out["statistics_usage"], snapshot["revision"]);
    }
    let mut rdf = r.clone();
    rdf["language"] = json!("sparql");
    rdf["query"] = json!("SELECT ?name WHERE {?s <urn:name> ?name FILTER(?name=\"rare\")}");
    rdf["rdf"] = json!([{"table":"people","subject":{"kind":"template","prefix":"urn:person:","columns":["id"]},"predicate":{"kind":"constant","value":"urn:name"},"object":{"kind":"literal","column":"name"}}]);
    let out = cmd(json!({"op":"compile","catalog_id":finished["catalog_id"],"request":rdf})).await;
    assert!(!out["plan_estimates"].as_array().unwrap().is_empty());
    let loaded = cmd(json!({"op":"install","snapshot":snapshot})).await;
    cmd(json!({"op":"release","catalog_id":loaded["catalog_id"]})).await;
    cmd(json!({"op":"release","catalog_id":finished["catalog_id"]})).await;
    assert!(
        statistics::command(
            &json!({"op":"compile","catalog_id":finished["catalog_id"],"request":r}).to_string()
        )
        .await
        .is_err()
    );
}
#[test]
fn bounded_collection_and_partial_reports() {
    let mut g = Generator::new(request()).unwrap();
    let t = g.next().unwrap();
    assert_eq!(t.kind, "metadata");
    g.submit(&t.id, vec![], Some("unsupported".into()), true)
        .unwrap();
    let t = g.next().unwrap();
    assert!(t.sql.contains("LIMIT"));
    g.submit(
        &t.id,
        vec![serde_json::from_value(json!({"id":1,"name":"x"})).unwrap()],
        None,
        false,
    )
    .unwrap();
    g.submit(&t.id, vec![], Some("cancelled read".into()), true)
        .unwrap();
    let s = g.finish();
    assert!(!s.report.complete);
    assert_eq!(s.sources["people"].sample_rows, 1);
    assert_ne!(s.sources["people"].method, "complete bounded read");
    assert!(s.sources["people"].estimated_rows.is_none());
}
#[tokio::test]
async fn absent_statistics_preserves_plan_and_no_diagnostics() {
    let out: Value =
        serde_json::from_str(&compile_json(&request().to_string()).await.unwrap()).unwrap();
    assert_eq!(out["plan_estimates"], json!([]));
    assert!(out["statistics_usage"].is_null());
}

#[cfg(feature = "duckdb")]
fn demo() -> (duckdb::Connection, Value) {
    let db = duckdb::Connection::open_in_memory().unwrap();
    db.execute_batch("CREATE TABLE people(id BIGINT,name VARCHAR); INSERT INTO people SELECT i,CASE WHEN i=0 THEN 'rare' ELSE 'common' END FROM range(100) t(i); CREATE TABLE totals AS SELECT name,count(*)::BIGINT AS n FROM people GROUP BY name;").unwrap();
    let mut r = request();
    r["tables"].as_array_mut().unwrap().push(json!({"name":"totals","columns":[{"name":"name","data_type":"string"},{"name":"n","data_type":"int64"}]}));
    r["representation_sources"] = json!([{"name":"summary","default_representation":"grouped","representations":[{"name":"grouped","source":{"kind":"query","sql":"SELECT name,count(*) AS n FROM people GROUP BY name"}},{"name":"materialized","source":{"kind":"table","name":"totals"}}]}]);
    r["nodes"] = json!([{"label":"Summary","table":"summary","id":"name","properties":{"name":"name","n":"n"}}]);
    r["query"] = json!("MATCH (s:Summary) RETURN s.name,s.n ORDER BY s.name");
    (db, r)
}
#[cfg(feature = "duckdb")]
#[tokio::test]
async fn generated_statistics_choose_cheaper_materialization_with_identical_answers() {
    let (db, mut r) = demo();
    let before: Value = serde_json::from_str(&compile_json(&r.to_string()).await.unwrap()).unwrap();
    assert_eq!(
        before["representation_selections"][0]["representation"],
        "grouped"
    );
    let snapshot = statistics::generate_duckdb(&db, r.clone()).unwrap();
    assert_eq!(snapshot.sources["people"].sample_rows, 100);
    assert_eq!(snapshot.sources["totals"].sample_rows, 2);
    r["statistics"] = serde_json::to_value(&snapshot).unwrap();
    let after: Value = serde_json::from_str(&compile_json(&r.to_string()).await.unwrap()).unwrap();
    assert_eq!(
        after["representation_selections"][0]["representation"], "materialized",
        "{after}"
    );
    let rows = |q: &Value| {
        let mut s = db.prepare(q["sql"].as_str().unwrap()).unwrap();
        s.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
    };
    assert_eq!(rows(&before), rows(&after));
    let candidates = after["representation_selections"][0]["candidates"]
        .as_array()
        .unwrap();
    assert!(
        candidates[1]["estimated_cost"].as_u64().unwrap()
            < candidates[0]["estimated_cost"].as_u64().unwrap()
    );
}
#[cfg(feature = "duckdb")]
#[test]
fn collection_summaries_and_composite_edges() {
    let db = duckdb::Connection::open_in_memory().unwrap();
    db.execute_batch("CREATE TABLE e(a BIGINT,b BIGINT,c BIGINT,items STRUCT(sku VARCHAR,qty BIGINT)[]); INSERT INTO e VALUES (1,1,2,[{'sku':'x','qty':2},{'sku':'x','qty':3}]),(1,2,3,[]),(2,1,3,NULL)").unwrap();
    let r = json!({"dialect":"duckdb","tables":[{"name":"e","columns":[{"name":"a","data_type":"int64"},{"name":"b","data_type":"int64"},{"name":"c","data_type":"int64"},{"name":"items","data_type":"list:struct:{\"sku\":\"string\",\"qty\":\"int64\"}"}]}],"edges":[{"label":"E","table":"e","source":["a","b"],"target":"c"}]});
    let s = statistics::generate_duckdb(&db, r).unwrap();
    let list = s.sources["e"].columns["items"].list.as_ref().unwrap();
    assert_eq!(list.empty_lists, 1);
    assert_eq!(list.null_lists, 1);
    assert_eq!(list.total_lengths, 2);
    assert_eq!(list.parents_containing["sku:\"x\""], 1);
    assert_eq!(s.relationships[0].source_distinct, 3);
    assert_eq!(s.relationships[0].target_distinct, 2);
}

#[cfg(feature = "duckdb")]
#[tokio::test]
async fn engine_statistics_reuse_clear_and_filter_order() {
    use arrow::datatypes::{DataType, Field, Schema};
    use orchiddb::{
        engine::GraphEngine,
        ir::rel::mapping::{GraphMapping, NodeMapping},
    };
    use std::sync::Arc;
    let db = duckdb::Connection::open_in_memory().unwrap();
    db.execute_batch("CREATE TABLE people(id BIGINT,name VARCHAR);INSERT INTO people SELECT i,CASE WHEN i=1 THEN 'rare' ELSE 'common' END FROM range(100) t(i)").unwrap();
    let mut m = GraphMapping::new();
    m.register_table_schema(
        "people",
        Arc::new(Schema::new(vec![
            Field::new("id", DataType::Int64, true),
            Field::new("name", DataType::Utf8, true),
        ])),
    );
    m.map_node(
        NodeMapping::table("N", "people", "id")
            .property("name", "name")
            .property("id", "id"),
    );
    let mut engine = GraphEngine::mapped(db, Arc::new(m)).unwrap();
    let snapshot = engine.generate_statistics().unwrap();
    assert_eq!(snapshot.sources["people"].sample_rows, 100);
    let out = engine
        .cypher("MATCH (n:N) WHERE n.id>=0 AND n.name='rare' RETURN n.id")
        .await
        .unwrap();
    assert_eq!(out.returned.batch.num_rows(), 1);
    assert!(!out.stats.plan_estimates.is_empty());
    engine.clear_statistics().unwrap();
    let out = engine
        .cypher("MATCH (n:N) WHERE n.name='rare' RETURN n.id")
        .await
        .unwrap();
    assert_eq!(out.returned.batch.num_rows(), 1);
    assert!(out.stats.plan_estimates.is_empty());
    engine.begin().unwrap();
    engine.generate_statistics().unwrap();
    engine.rollback().unwrap();
    assert!(engine.statistics().is_none());
    engine.begin().unwrap();
    engine.generate_statistics().unwrap();
    engine.commit().unwrap();
    assert!(engine.statistics().is_none());
}

#[cfg(feature = "duckdb")]
#[test]
fn pure_filter_order_and_joint_estimates_are_actionable() {
    use arrow::datatypes::{DataType, Field, Schema};
    use orchiddb::ir::rel::mapping::GraphMapping;
    use std::sync::Arc;
    let db = duckdb::Connection::open_in_memory().unwrap();
    db.execute_batch(
        "CREATE TABLE t(a BIGINT,b BIGINT);INSERT INTO t SELECT i%10,i%10 FROM range(1000) t(i)",
    )
    .unwrap();
    let r = json!({"dialect":"duckdb","tables":[{"name":"t","columns":[{"name":"a","data_type":"int64"},{"name":"b","data_type":"int64"}]}]});
    let snapshot = statistics::generate_duckdb(&db, r).unwrap();
    let mut m = GraphMapping::new();
    m.register_table_schema(
        "t",
        Arc::new(Schema::new(vec![
            Field::new("a", DataType::Int64, true),
            Field::new("b", DataType::Int64, true),
        ])),
    );
    m.set_statistics(Arc::new(snapshot)).unwrap();
    let p = m
        .relational_plan("SELECT a FROM t WHERE a>=0 AND b=1")
        .unwrap();
    let (_, decisions) = statistics::optimize(p).unwrap();
    assert_eq!(decisions.len(), 1);
    assert!(decisions[0].estimated_work_after < decisions[0].estimated_work_before);
    let p = m
        .relational_plan("SELECT a FROM t WHERE a=1 AND b=1")
        .unwrap();
    assert_eq!(statistics::estimate(&p).estimated_rows, Some(100.0));
}

#[tokio::test]
async fn invalid_ipc_does_not_poison_registry_and_truncation_stays_partial() {
    let start = cmd(json!({"op":"begin","request":request()})).await;
    let id = start["id"].clone();
    assert!(
        statistics::command(
            &json!({"op":"submit","id":id,"request_id":start["request"]["id"],"ipc":"AAAA"})
                .to_string()
        )
        .await
        .is_err()
    );
    let next = cmd(json!({"op":"next","id":id})).await;
    assert_eq!(next, start);
    let next = cmd(
        json!({"op":"submit","id":id,"request_id":next["request"]["id"],"rows":[{"rows":1000}]}),
    )
    .await;
    cmd(json!({"op":"submit","id":id,"request_id":next["request"]["id"],"rows":[{"id":1,"name":"x"}],"truncated":true})).await;
    let out = cmd(json!({"op":"finish","id":id})).await;
    assert_eq!(
        out["snapshot"]["sources"]["people"]["estimated_rows"],
        1000.0
    );
    assert_eq!(out["report"]["complete"], false);
    cmd(json!({"op":"release","catalog_id":out["catalog_id"]})).await;
}

#[cfg(feature = "duckdb")]
#[tokio::test]
async fn collected_nested_expansion_selects_flat_without_manual_statistics() {
    let db = duckdb::Connection::open_in_memory().unwrap();
    db.execute_batch("CREATE TABLE orders AS SELECT i::BIGINT AS id, i::BIGINT AS customer_id, list_transform(range(20), x -> {'item_id': x, 'sku': 'sku', 'quantity': 1::BIGINT}) AS items FROM range(100) t(i); CREATE TABLE flat AS SELECT id AS oid, unnest(items).item_id AS line, unnest(items).sku AS product, unnest(items).quantity AS qty FROM orders").unwrap();
    let mut r: Value =
        serde_json::from_str(include_str!("../examples/data/representation_sources.json")).unwrap();
    for rep in r["representation_sources"][0]["representations"]
        .as_array_mut()
        .unwrap()
    {
        rep.as_object_mut().unwrap().remove("statistics");
        rep.as_object_mut().unwrap().remove("average_list_length");
    }
    r["query"] = json!("MATCH (i:Item) RETURN i.sku");
    let before: Value = serde_json::from_str(&compile_json(&r.to_string()).await.unwrap()).unwrap();
    assert_eq!(
        before["representation_selections"][0]["representation"],
        "nested"
    );
    let snapshot = statistics::generate_duckdb(&db, r.clone()).unwrap();
    r["statistics"] = serde_json::to_value(&snapshot).unwrap();
    let after: Value = serde_json::from_str(&compile_json(&r.to_string()).await.unwrap()).unwrap();
    assert_eq!(
        after["representation_selections"][0]["representation"], "flat_sku",
        "{after}"
    );
    let candidates = &after["representation_selections"][0]["candidates"];
    assert_eq!(
        candidates[0]["estimated_expanded_rows"], 2000,
        "{candidates}"
    );
    for plan in [&before, &after] {
        let mut statement = db.prepare(plan["sql"].as_str().unwrap()).unwrap();
        let batches = statement.query_arrow([]).unwrap().collect::<Vec<_>>();
        assert_eq!(batches.iter().map(|b| b.num_rows()).sum::<usize>(), 2000);
    }
}
#[cfg(feature = "duckdb")]
#[tokio::test]
async fn cheaper_materialization_for_gremlin_and_sparql() {
    let (db, r) = demo();
    let snapshot = statistics::generate_duckdb(&db, r.clone()).unwrap();
    for (language, query) in [
        ("gremlin", "g.V().hasLabel('Summary').values('n').order()"),
        ("sparql", "SELECT ?n WHERE { ?s <urn:n> ?n } ORDER BY ?n"),
    ] {
        let mut q = r.clone();
        q["language"] = json!(language);
        q["query"] = json!(query);
        q["rdf"] = json!([{"table":"summary","subject":{"kind":"template","prefix":"urn:summary:","columns":["name"]},"predicate":{"kind":"constant","value":"urn:n"},"object":{"kind":"literal","column":"n"}}]);
        let before: Value =
            serde_json::from_str(&compile_json(&q.to_string()).await.unwrap()).unwrap();
        q["statistics"] = serde_json::to_value(&snapshot).unwrap();
        let after: Value =
            serde_json::from_str(&compile_json(&q.to_string()).await.unwrap()).unwrap();
        assert_eq!(
            after["representation_selections"][0]["representation"], "materialized",
            "{after}"
        );
        let rows = |plan: &Value| {
            let mut stmt = db.prepare(plan["sql"].as_str().unwrap()).unwrap();
            let batches = stmt.query_arrow([]).unwrap().collect::<Vec<_>>();
            arrow::util::pretty::pretty_format_batches(&batches)
                .unwrap()
                .to_string()
        };
        assert_eq!(rows(&before), rows(&after), "{language}");
    }
}
#[cfg(feature = "duckdb")]
#[test]
fn wide_catalog_gets_samples_and_large_sources_get_block_sampling() {
    let mut r = request();
    r["tables"] = json!(
        (0..20)
            .map(|i| json!({"name":format!("t{i}"),"columns":[{"name":"id","data_type":"int64"}]}))
            .collect::<Vec<_>>()
    );
    let mut g = Generator::new(r).unwrap();
    let mut samples = 0;
    while let Some(t) = g.next() {
        assert_eq!(t.kind, "sample");
        samples += 1;
        g.submit(&t.id, vec![], None, true).unwrap();
    }
    assert_eq!(samples, 20);
    let db = duckdb::Connection::open_in_memory().unwrap();
    db.execute_batch(
        "CREATE TABLE people AS SELECT i::BIGINT id,'x'::VARCHAR AS name FROM range(100000) t(i)",
    )
    .unwrap();
    let s = statistics::generate_duckdb(&db, request()).unwrap();
    assert!(s.sources["people"].method.starts_with("system"));
    assert!(s.report.skipped.is_empty(), "{:?}", s.report);
    assert!(s.sources["people"].sample_rows <= 8192);
}

#[cfg(feature = "duckdb")]
#[test]
fn tuple_join_estimates_preserve_skew_and_distinct_correlation() {
    use arrow::datatypes::{DataType, Field, Schema};
    use orchiddb::ir::rel::mapping::GraphMapping;
    use std::sync::Arc;
    let db = duckdb::Connection::open_in_memory().unwrap();
    db.execute_batch("CREATE TABLE l AS SELECT CASE WHEN i<90 THEN 1 ELSE 2 END::BIGINT a, CASE WHEN i<90 THEN 1 ELSE 2 END::BIGINT b FROM range(100) t(i);CREATE TABLE r AS SELECT * FROM l;").unwrap();
    let request = json!({"tables":[{"name":"l","columns":[{"name":"a","data_type":"int64"},{"name":"b","data_type":"int64"}]},{"name":"r","columns":[{"name":"a","data_type":"int64"},{"name":"b","data_type":"int64"}]}]});
    let snapshot = statistics::generate_duckdb(&db, request).unwrap();
    let mut mapping = GraphMapping::new();
    for name in ["l", "r"] {
        mapping.register_table_schema(
            name,
            Arc::new(Schema::new(vec![
                Field::new("a", DataType::Int64, true),
                Field::new("b", DataType::Int64, true),
            ])),
        );
    }
    mapping.set_statistics(Arc::new(snapshot)).unwrap();
    let plan = mapping
        .relational_plan("SELECT l.a FROM l JOIN r ON l.a=r.a AND l.b=r.b")
        .unwrap();
    let e = statistics::estimate(&plan);
    assert_eq!(e.estimated_rows.unwrap().round(), 8200.0);
    let plan = mapping
        .relational_plan("SELECT DISTINCT a,b FROM l")
        .unwrap();
    assert_eq!(statistics::estimate(&plan).estimated_rows, Some(2.0));
}

#[tokio::test]
async fn layouts_use_collected_physical_costs_without_partition_bounds() {
    let mut r = request();
    let mut table = r["tables"][0].clone();
    table["name"] = json!("compact");
    r["tables"].as_array_mut().unwrap().push(table);
    r["logical_sources"] = json!([{"name":"logical_people","default_table":"people","layouts":[{"table":"people","partitions":null},{"table":"compact","partitions":null}]}]);
    r["nodes"][0]["table"] = json!("logical_people");
    let mut g = Generator::new(r.clone()).unwrap();
    while let Some(t) = g.next() {
        let rows = if t.kind == "metadata" {
            vec![
                serde_json::from_value(
                    json!({"rows":2,"bytes":if t.source=="people"{100000}else{1000}}),
                )
                .unwrap(),
            ]
        } else {
            vec![
                serde_json::from_value(json!({"id":1,"name":"rare"})).unwrap(),
                serde_json::from_value(json!({"id":2,"name":"common"})).unwrap(),
            ]
        };
        g.submit(&t.id, rows, None, true).unwrap();
    }
    r["statistics"] = serde_json::to_value(g.finish()).unwrap();
    let result: Value = serde_json::from_str(&compile_json(&r.to_string()).await.unwrap()).unwrap();
    assert_eq!(result["layout_selections"][0]["table"], "compact");
    assert!(result["layout_selections"][0]["estimated_files"].is_null());
    assert!(result["sql"].as_str().unwrap().contains("rare"));
}

#[test]
fn unsupported_nested_projection_is_unknown_not_all_null() {
    let mut g=Generator::new(json!({"dialect":"postgres","tables":[{"name":"p","columns":[{"name":"id","data_type":"int64"},{"name":"items","data_type":"list:int64"}]}]})).unwrap();
    let t = g.next().unwrap();
    g.submit(&t.id, vec![], Some("metadata unavailable".into()), true)
        .unwrap();
    let t = g.next().unwrap();
    assert!(!t.sql.contains("items"));
    g.submit(
        &t.id,
        vec![serde_json::from_value(json!({"id":1})).unwrap()],
        None,
        true,
    )
    .unwrap();
    let s = g.finish();
    assert!(!s.sources["p"].columns.contains_key("items"));
    assert!(!s.report.notes.is_empty());
}
