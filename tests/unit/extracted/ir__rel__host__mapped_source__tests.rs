
    use super::*;
    use crate::engine::mapped_source::attach;

    #[test]
    fn empty_source_confirmation_follows_a_miss_and_is_cached() {
        let db = duckdb::Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE people(id BIGINT, name VARCHAR)").unwrap();
        let mut mapping = GraphMapping::new();
        mapping.map_node(crate::ir::rel::mapping::NodeMapping::table("Person", "people", "id").property("name", "name"));
        let graph = attach(Arc::new(Mutex::new(DuckDbExecutor::from_connection(db))), Arc::new(mapping)).unwrap();
        let source = graph.source.unwrap();
        for id in 1_i64..=3 {
            assert_eq!(source.property(false, "Person", &id.into(), "name"), Value::Null);
        }
        source.check().unwrap();
        let queries = source.stats().1;
        assert_eq!(queries.len(), 2, "one missed lookup, one confirmation, then cached emptiness: {queries:?}");
        assert!(!queries[0].starts_with("SELECT 1"));
        assert!(queries[1].starts_with("SELECT 1"));
    }

    #[test]
    fn native_partition_reads_preserve_endpoints_properties_and_adjacency() {
        use crate::ir::rel::mapping::{NodeMapping, EdgeMapping};
        let db=duckdb::Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE a(id BIGINT); CREATE TABLE b(id BIGINT); CREATE TABLE c(id BIGINT); INSERT INTO a VALUES(1); INSERT INTO b VALUES(1); INSERT INTO c VALUES(1); CREATE TABLE ab(id BIGINT,src BIGINT,dst BIGINT,p VARCHAR); CREATE TABLE ac(id BIGINT,src BIGINT,dst BIGINT,q VARCHAR); INSERT INTO ab VALUES(10,1,1,'ab'),(11,1,1,'parallel'); INSERT INTO ac VALUES(12,1,1,'ac')").unwrap();
        let mut mapping=GraphMapping::new();
        for (label,table) in [("A","a"),("B","b"),("C","c")] {mapping.map_node(NodeMapping::table(label,table,"id"));}
        mapping.map_edge_partition(EdgeMapping::table("R","ab","src","dst","A","B").with_id("id").property("p","p"));
        mapping.map_edge_partition(EdgeMapping::table("R","ac","src","dst","A","C").with_id("id").property("q","q"));
        let host=Arc::new(Mutex::new(DuckDbExecutor::from_connection(db)));
        let graph=attach(host,Arc::new(mapping)).unwrap();
        let source=graph.source.as_ref().unwrap();
        let mut ids=source.ids(true,"R"); ids.sort();
        assert_eq!(ids,vec![ElementId::from(10i64),ElementId::from(11i64),ElementId::from(12i64)]);
        assert_eq!(source.property(true,"R",&12i64.into(),"q"),Value::String("ac".into()));
        assert_eq!(source.property(true,"R",&12i64.into(),"p"),Value::Null);
        assert_eq!(source.endpoints("R",&12i64.into()),Some(("A".into(),1i64.into(),"C".into(),1i64.into())));
        assert_eq!(source.neighbors(false,"A",&1i64.into(),&[]).len(),3);
        assert_eq!(source.neighbors(true,"B",&1i64.into(),&[]).len(),2);
        assert_eq!(source.neighbors(true,"C",&1i64.into(),&[]).len(),1);
        source.check().unwrap();
    }

    #[test]
    fn borrowed_prefetch_finds_nested_elements_and_reuses_cached_records() {
        let db = duckdb::Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE people(id BIGINT, name VARCHAR); INSERT INTO people VALUES (1, 'one'), (2, 'two'); CREATE TABLE links(id BIGINT, src BIGINT, dst BIGINT); INSERT INTO links VALUES (7, 1, 2)").unwrap();
        let mut mapping = GraphMapping::new();
        mapping.register_table_schema(
            "people",
            Arc::new(Schema::new(vec![
                Field::new("id", arrow::datatypes::DataType::Int64, true),
                Field::new("name", arrow::datatypes::DataType::Utf8, true),
            ])),
        );
        mapping.register_table_schema(
            "links",
            Arc::new(Schema::new(
                ["id", "src", "dst"]
                    .map(|name| Field::new(name, arrow::datatypes::DataType::Int64, true))
                    .to_vec(),
            )),
        );
        mapping.map_node(
            crate::ir::rel::mapping::NodeMapping::table("Person", "people", "id")
                .property("name", "name"),
        );
        let mut edge = crate::ir::rel::mapping::EdgeMapping::table(
            "LINK", "links", "src", "dst", "Person", "Person",
        );
        edge.id_column = Some("id".into());
        mapping.map_edge(edge);
        let graph = attach(
            Arc::new(Mutex::new(DuckDbExecutor::from_connection(db))),
            Arc::new(mapping),
        )
        .unwrap();
        let node = |id| Value::Node {
            label: "Person".into(),
            id: ElementId::from(id),
        };
        let edge = Value::Edge {
            rel_type: "LINK".into(),
            id: 7i64.into(),
            src_label: "Person".into(),
            src_id: 1i64.into(),
            dst_label: "Person".into(),
            dst_id: 2i64.into(),
            projected_properties: None,
        };
        let rows = vec![crate::ir::runtime::Row::new().with(
            "nested",
            Value::Map(BTreeMap::from([
                (
                    "elements".into(),
                    Value::List(vec![
                        node(1i64),
                        Value::Path(vec![node(2i64), edge]),
                        node(1i64),
                    ]),
                ),
                ("payload".into(), Value::String("x".repeat(262_144))),
            ])),
        )];
        let source = graph.source.as_ref().unwrap();
        let before = source.stats().1.len();
        graph.prefetch_source(&rows);
        source.check().unwrap();
        let after = source.stats().1.len();
        assert_eq!(after - before, 2, "one batched fetch per element type");
        assert_eq!(
            source.property(false, "Person", &1i64.into(), "name"),
            Value::String("one".into())
        );
        assert_eq!(
            source.property(false, "Person", &2i64.into(), "name"),
            Value::String("two".into())
        );
        assert_eq!(
            source.endpoints("LINK", &7i64.into()),
            Some(("Person".into(), 1i64.into(), "Person".into(), 2i64.into()))
        );
        graph.prefetch_source(&rows);
        graph.prefetch_source(&[]);
        source.check().unwrap();
        assert_eq!(source.stats().1.len(), after);
    }

    #[test]
    fn statistics_choose_dense_scan_and_sparse_lookup() {
        for (statistics, count, expected_queries, derived) in [
            (false, 3000, 3, false),
            (true, 3000, 1, false),
            (true, 2, 1, false),
            (true, 3000, 1, true),
        ] {
            let db = duckdb::Connection::open_in_memory().unwrap();
            db.execute_batch("CREATE TABLE people AS SELECT i::BIGINT id, 'name-'||i AS name FROM range(5000) t(i)").unwrap();
            let mut mapping = GraphMapping::new();
            mapping.register_table_schema(
                "people",
                Arc::new(Schema::new(vec![
                    Field::new("id", arrow::datatypes::DataType::Int64, true),
                    Field::new("name", arrow::datatypes::DataType::Utf8, true),
                ])),
            );
            mapping.map_node(if derived {
                crate::ir::rel::mapping::NodeMapping::query(
                    "Person",
                    "SELECT id,name FROM people",
                    "id",
                )
                .property("name", "name")
            } else {
                crate::ir::rel::mapping::NodeMapping::table("Person", "people", "id")
                    .property("name", "name")
            });
            if statistics {
                let snapshot = crate::ir::rel::statistics::generate_duckdb(
                    &db,
                    mapping.statistics_request().unwrap(),
                )
                .unwrap();
                mapping.set_statistics(Arc::new(snapshot)).unwrap();
            }
            let graph = attach(
                Arc::new(Mutex::new(DuckDbExecutor::from_connection(db))),
                Arc::new(mapping),
            )
            .unwrap();
            let source = graph.source.unwrap();
            let values = (0..count)
                .map(|i| Value::Node {
                    label: "Person".into(),
                    id: ElementId::new(ScalarValue::Int64(Some(i))).unwrap(),
                })
                .collect::<Vec<_>>();
            source.prefetch(&mut values.iter());
            source.check().unwrap();
            assert_eq!(
                source.stats().1.len(),
                expected_queries,
                "{:?}",
                source.stats()
            );
            assert_eq!(
                source.access_decisions().len(),
                usize::from(statistics && count > 1024)
            );
            for i in [0, count - 1] {
                assert_eq!(
                    source.property(
                        false,
                        "Person",
                        &ElementId::new(ScalarValue::Int64(Some(i))).unwrap(),
                        "name"
                    ),
                    Value::String(format!("name-{i}"))
                );
            }
            source.prefetch(&mut values.iter());
            assert_eq!(source.stats().1.len(), expected_queries);
        }
    }

    #[test]
    fn stale_statistics_scan_cap_falls_back_once() {
        let db = duckdb::Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE people AS SELECT i::BIGINT id FROM range(5000) t(i)")
            .unwrap();
        let mut mapping = GraphMapping::new();
        mapping.register_table_schema(
            "people",
            Arc::new(Schema::new(vec![Field::new(
                "id",
                arrow::datatypes::DataType::Int64,
                true,
            )])),
        );
        mapping.map_node(crate::ir::rel::mapping::NodeMapping::table(
            "Person", "people", "id",
        ));
        let snapshot =
            crate::ir::rel::statistics::generate_duckdb(&db, mapping.statistics_request().unwrap())
                .unwrap();
        mapping.set_statistics(Arc::new(snapshot)).unwrap();
        db.execute_batch("INSERT INTO people SELECT i FROM range(5000,70000) t(i)")
            .unwrap();
        let graph = attach(
            Arc::new(Mutex::new(DuckDbExecutor::from_connection(db))),
            Arc::new(mapping),
        )
        .unwrap();
        let source = graph.source.unwrap();
        let values = |start| {
            (start..start + 3000)
                .map(|i| Value::Node {
                    label: "Person".into(),
                    id: ElementId::new(ScalarValue::Int64(Some(i))).unwrap(),
                })
                .collect::<Vec<_>>()
        };
        source.prefetch(&mut values(0).iter());
        source.check().unwrap();
        assert_eq!(source.stats().1.len(), 4);
        assert_eq!(
            source.access_decisions()[0].after,
            vec!["batched_lookup_after_scan_cap"]
        );
        source.prefetch(&mut values(5000).iter());
        source.check().unwrap();
        assert_eq!(
            source.stats().1.len(),
            7,
            "a capped scan must not be retried"
        );
        assert!(source.exists(
            false,
            "Person",
            &ElementId::new(ScalarValue::Int64(Some(7999))).unwrap()
        ));
    }

    #[test]
    fn dense_adjacency_scan_preserves_parallel_edges_and_direction() {
        let db = duckdb::Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE people AS SELECT i::BIGINT id FROM range(5000) t(i); CREATE TABLE links AS SELECT i::BIGINT id,(i/2)::BIGINT src,((i/2+1)%5000)::BIGINT dst FROM range(10000) t(i)").unwrap();
        let mut mapping = GraphMapping::new();
        for (table, columns) in [("people", vec!["id"]), ("links", vec!["id", "src", "dst"])] {
            mapping.register_table_schema(
                table,
                Arc::new(Schema::new(
                    columns
                        .into_iter()
                        .map(|c| Field::new(c, arrow::datatypes::DataType::Int64, true))
                        .collect::<Vec<_>>(),
                )),
            );
        }
        mapping.map_node(crate::ir::rel::mapping::NodeMapping::table(
            "Person", "people", "id",
        ));
        let mut edge = crate::ir::rel::mapping::EdgeMapping::table(
            "LINK", "links", "src", "dst", "Person", "Person",
        );
        edge.id_column = Some("id".into());
        mapping.map_edge(edge);
        let snapshot =
            crate::ir::rel::statistics::generate_duckdb(&db, mapping.statistics_request().unwrap())
                .unwrap();
        mapping.set_statistics(Arc::new(snapshot)).unwrap();
        let graph = attach(
            Arc::new(Mutex::new(DuckDbExecutor::from_connection(db))),
            Arc::new(mapping),
        )
        .unwrap();
        let source = graph.source.unwrap();
        let id = |i| ElementId::new(ScalarValue::Int64(Some(i))).unwrap();
        let nodes = (0..3000)
            .map(|i| ("Person".into(), id(i)))
            .collect::<Vec<_>>();
        source.prefetch_neighbors(false, &nodes, &["LINK".into()]);
        source.check().unwrap();
        assert_eq!(source.stats().1.len(), 1);
        let outgoing = source.neighbors(false, "Person", &id(10), &["LINK".into()]);
        assert!(
            outgoing.len() >= 2,
            "parallel edges were lost: {outgoing:?}"
        );
        source.prefetch_neighbors(true, &nodes, &["LINK".into()]);
        source.check().unwrap();
        assert_eq!(source.stats().1.len(), 1);
        assert!(
            !source
                .neighbors(true, "Person", &id(11), &["LINK".into()])
                .is_empty()
        );
        assert_eq!(
            outgoing,
            source.neighbors(false, "Person", &id(10), &["LINK".into()])
        );
    }

    #[test]
    fn jvm_views_allocate_property_handles_lazily_and_consistently() {
        let db = duckdb::Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE people(id VARCHAR PRIMARY KEY, name VARCHAR); INSERT INTO people VALUES ('a','Alice'),('b','Bob'); CREATE VIEW unrelated AS SELECT error('unexpected scan')::VARCHAR AS id FROM range(1)").unwrap();
        let mut mapping = GraphMapping::new();
        mapping.map_node(
            crate::ir::rel::mapping::NodeMapping::table("Person", "people", "id")
                .property("name", "name"),
        );
        mapping.map_node(crate::ir::rel::mapping::NodeMapping::table(
            "Unrelated",
            "unrelated",
            "id",
        ));
        let graph = attach(
            Arc::new(Mutex::new(DuckDbExecutor::from_connection(db))),
            Arc::new(mapping),
        )
        .unwrap();
        let source = graph.source.clone().unwrap();
        let mut store = crate::jvm_bridge::Store::from_execution_graph(graph);
        assert_eq!(
            source.stats().0,
            0,
            "JVM construction must not fetch graph rows"
        );
        let a = Value::Node {
            label: "Person".into(),
            id: ElementId::new(ScalarValue::Utf8(Some("a".into()))).unwrap(),
        };
        let b = Value::Node {
            label: "Person".into(),
            id: ElementId::new(ScalarValue::Utf8(Some("b".into()))).unwrap(),
        };
        let checkpoint = store.graph().clone();
        let writer_b = store.graph().properties(&b, &["name".into()]);
        let reader_a = checkpoint.properties(&a, &["name".into()]);
        let writer_a = store.graph().properties(&a, &["name".into()]);
        assert_eq!(reader_a, writer_a);
        assert_eq!(writer_b, checkpoint.properties(&b, &["name".into()]));
        // Committing a writer that allocated in a different order must retain
        // the same identities seen by readers of the prior checkpoint.
        let response = store.request(&serde_json::json!({"op":"commit"}));
        assert_eq!(response["ok"], true, "{response}");
        assert_eq!(writer_a, store.graph().properties(&a, &["name".into()]));
        source.check().unwrap();
        assert_eq!(source.stats().0, 2);
        assert!(
            source
                .stats()
                .1
                .iter()
                .all(|sql| !sql.contains("unrelated"))
        );
    }

fn generated_cypher_edge_id(id: u64) -> ElementId {
    ElementId::new(datafusion::common::ScalarValue::UInt64(Some(id))).unwrap()
}

fn cypher_relationship_source_fixture() -> crate::ir::catalog::PropertyGraph {
    use crate::ir::rel::mapping::NodeMapping;
    let db=duckdb::Connection::open_in_memory().unwrap();
    db.execute_batch("CREATE TABLE people(id BIGINT,score BIGINT); INSERT INTO people VALUES(1,10),(2,20),(3,30)").unwrap();
    let mut mapping=GraphMapping::new();
    mapping.register_table_schema("people",Arc::new(Schema::new(vec![Field::new("id",arrow::datatypes::DataType::Int64,false),Field::new("score",arrow::datatypes::DataType::Int64,false)])));
    mapping.map_node(NodeMapping::table("Person","people","id").property("id","id").property("score","score"));
    let rule=serde_json::from_value(serde_json::json!({"name":"NEAR","source":"Person","target":"Person",
        "cypher":"WITH source MATCH (target:Person) WHERE target.id <> source.id RETURN target, target.score AS score ORDER BY target.id LIMIT 1",
        "returns":{"target":"target","properties":{"score":{"type":"integer"}}}})).unwrap();
    mapping.map_cypher_relationship(rule).unwrap();
    attach(Arc::new(Mutex::new(DuckDbExecutor::from_connection(db))),Arc::new(mapping)).unwrap()
}

#[test]
fn native_cypher_relationship_ids_hydrate_typed_endpoints_and_cache_properties() {
    let graph=cypher_relationship_source_fixture();let source=graph.source.as_ref().unwrap();
    let mut ids=source.ids(true,"NEAR");source.check().unwrap();ids.sort();
    assert_eq!(ids,vec![generated_cypher_edge_id(1),generated_cypher_edge_id(2),generated_cypher_edge_id(3)]);
    let queries=source.stats().1.len();
    for (edge,src,dst,score) in [(1_i64,1_i64,2_i64,20_i64),(2,2,1,10),(3,3,1,10)] {
        assert_eq!(source.endpoints("NEAR",&generated_cypher_edge_id(edge as u64)),Some(("Person".into(),src.into(),"Person".into(),dst.into())));
        assert_eq!(source.property(true,"NEAR",&generated_cypher_edge_id(edge as u64),"score"),Value::Int(score));
        assert_eq!(source.property(true,"NEAR",&generated_cypher_edge_id(edge as u64),"missing"),Value::Null);
    }
    source.check().unwrap();assert_eq!(source.stats().1.len(),queries);
}

#[test]
fn native_cypher_relationship_adjacency_prefetch_preserves_direction_and_cache() {
    let graph=cypher_relationship_source_fixture();let source=graph.source.as_ref().unwrap();
    let types=vec!["NEAR".to_owned()];
    source.prefetch_neighbors(false,&[("Person".into(),1_i64.into()),("Person".into(),3_i64.into())],&types);
    source.check().unwrap();let queries=source.stats().1.len();
    for (id,edge,target) in [(1_i64,1_i64,2_i64),(3,3,1)] {
        assert_eq!(source.neighbors(false,"Person",&id.into(),&types),vec![("NEAR".into(),generated_cypher_edge_id(edge as u64),"Person".into(),target.into())]);
    }
    assert_eq!(source.stats().1.len(),queries);
    let mut incoming=source.neighbors(true,"Person",&1_i64.into(),&types);incoming.sort();source.check().unwrap();
    assert_eq!(incoming,vec![("NEAR".into(),generated_cypher_edge_id(2),"Person".into(),2_i64.into()),("NEAR".into(),generated_cypher_edge_id(3),"Person".into(),3_i64.into())]);
    source.invalidate();assert_eq!(source.neighbors(false,"Person",&1_i64.into(),&types).len(),1);source.check().unwrap();
}
