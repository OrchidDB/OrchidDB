
    use super::super::{legacy::ExecutorHost, observation::observe};
    use super::*;
    use crate::ir::{
        catalog::{Cardinality, import::import_graph},
        rel::sql::DuckDbExecutor,
    };
    use std::sync::Mutex;
    fn host() -> SharedHost {
        Arc::new(ExecutorHost::new(Arc::new(Mutex::new(
            DuckDbExecutor::from_connection(duckdb::Connection::open_in_memory().unwrap()),
        ))))
    }
    fn cypher(
        host: SharedHost,
        store: &ManagedStore,
        query: &str,
    ) -> Result<Vec<crate::ir::runtime::Row>, String> {
        use crate::ir::rel::runtime::{
            CompileOptions, KernelState, SubplanRunner, compile_for_host, host::tests::HostRunner,
        };
        let graph = store.attach(host.clone())?;
        let plan = crate::language::cypher::preparation::prepare(
            query,
            &BTreeMap::new(),
            Some(graph.procedures.as_ref()),
        )
        .map_err(|e| e.message)?;
        let compiled = compile_for_host(
            &plan,
            &graph,
            CompileOptions {
                sql_islands: false,
                rule_context: crate::ir::rel::runtime::host::tests::reference_context(),
                language_functions: true,
                ..Default::default()
            },
        )
        .map_err(|e| e.message)?;
        let runner = Arc::new(HostRunner::default());
        let mut state = KernelState::for_host(
            graph,
            runner.clone(),
            &crate::ir::rel::runtime::host::tests::reference_context(),
        );
        host.execute(HostRequest::new("BEGIN TRANSACTION"))?;
        let result = runner
            .run(&compiled, &mut state)
            .map_err(|e| e.to_string())
            .and_then(|rows| {
                store.persist(host.as_ref(), &state.graph)?;
                Ok(rows)
            });
        host.execute(HostRequest::new(if result.is_ok() {
            "COMMIT"
        } else {
            "ROLLBACK"
        }))?;
        result
    }
    #[test]
    fn managed_cypher_reuses_mutation_kernels_for_sequential_merge_and_labels() {
        let host = host();
        let store = ManagedStore::new("managed_graph");
        store.create(host.as_ref()).unwrap();
        let rows=cypher(host.clone(),&store,"CREATE (n:A {k:1}) WITH n MERGE (m:A {k:1}) ON MATCH SET m.hit=true RETURN id(n)=id(m) AS same").unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].get("same"), Value::Bool(true));
        let rows=cypher(host.clone(),&store,"MATCH (n:A) SET n:Employee:Active WITH n MATCH (m:Employee:Active) RETURN count(m) AS total").unwrap();
        assert_eq!(rows[0].get("total"), Value::Long(1));
        let rows = cypher(
            host.clone(),
            &store,
            "MATCH (n:Employee:Active) RETURN count(n) AS total",
        )
        .unwrap();
        assert_eq!(rows[0].get("total"), Value::Long(1));
        cypher(host.clone(), &store, "MATCH (n:Employee) REMOVE n:Active").unwrap();
        let rows = cypher(
            host.clone(),
            &store,
            "MATCH (n:Active) RETURN count(n) AS total",
        )
        .unwrap();
        assert_eq!(rows[0].get("total"), Value::Long(0));
        cypher(host.clone(),&store,"UNWIND [1,1,2] AS x MERGE (n:N {k:x}) ON CREATE SET n.created=x ON MATCH SET n.hits=coalesce(n.hits,0)+1 RETURN n.k AS k").unwrap();
        let rows = cypher(
            host.clone(),
            &store,
            "MATCH (n:N) RETURN n.k AS k,n.hits AS hits ORDER BY k",
        )
        .unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].get("hits"), Value::Int(1));
        let snapshot = observe(&store.attach(host).unwrap()).unwrap();
        assert_eq!(snapshot.nodes.len(), 3);
        assert_eq!(snapshot.labels.len(), 3);
    }
    #[test]
    fn managed_cypher_delete_path_detach_and_failed_statement_are_atomic() {
        let host = host();
        let store = ManagedStore::new("managed_graph");
        store.create(host.as_ref()).unwrap();
        cypher(host.clone(), &store, "CREATE (a:A)-[:R]->(b:B)").unwrap();
        let before =
            serde_json::to_value(observe(&store.attach(host.clone()).unwrap()).unwrap())
                .unwrap();
        assert!(cypher(host.clone(), &store, "MATCH (a:A) DELETE a").is_err());
        assert_eq!(
            serde_json::to_value(observe(&store.attach(host.clone()).unwrap()).unwrap())
                .unwrap(),
            before
        );
        assert!(cypher(host.clone(), &store, "CREATE (n:Bad) SET n.invalid=[{x:1}]").is_err());
        assert_eq!(
            serde_json::to_value(observe(&store.attach(host.clone()).unwrap()).unwrap())
                .unwrap(),
            before
        );
        cypher(host.clone(), &store, "MATCH p=(a:A)-[:R]->(b:B) DELETE p").unwrap();
        let snapshot = observe(&store.attach(host.clone()).unwrap()).unwrap();
        assert!(snapshot.nodes.is_empty());
        assert!(snapshot.relationships.is_empty());
        cypher(host.clone(), &store, "CREATE (:A)-[:R]->(:B)").unwrap();
        cypher(host.clone(), &store, "MATCH (a:A) DETACH DELETE a").unwrap();
        let snapshot = observe(&store.attach(host).unwrap()).unwrap();
        assert_eq!(snapshot.nodes.len(), 1);
        assert!(snapshot.relationships.is_empty());
    }
    #[test]
    fn nested_mutation_publishes_native_hydration_with_overlay() {
        let host = host();
        let store = ManagedStore::new("managed_graph");
        store.create(host.as_ref()).unwrap();
        let fixture = import_graph(&serde_json::json!({"nodes":[{"id":42,"label":"person","properties":{"name":"alice","_partition":"a"}}],"edges":[]})).unwrap();
        store.persist(host.as_ref(), &fixture).unwrap();
        let parent = store.attach(host.clone()).unwrap();
        let node = Value::Node {
            label: "person".into(),
            id: parent.node_ids("person").unwrap()[0].clone(),
        };
        // A correlated body is the first operation to hydrate this entity.
        let child = parent.clone();
        child
            .set_vertex_property(
                &node,
                "name",
                Value::String("bob".into()),
                Cardinality::Single,
                BTreeMap::new(),
            )
            .unwrap();
        parent.restore_execution_overlay(&child);
        let properties = parent.properties(&node, &["name".into()]);
        assert!(
            matches!(&properties[0], Value::VertexProperty { value, .. } if **value == Value::String("bob".into()))
        );
        store.persist(host.as_ref(), &parent).unwrap();
        let reopened = store.attach(host).unwrap();
        let properties = reopened.properties(&node, &["name".into()]);
        assert!(
            matches!(&properties[0], Value::VertexProperty { value, .. } if **value == Value::String("bob".into()))
        );
        let Value::Node { label, id } = node else {
            unreachable!()
        };
        assert_eq!(
            reopened.node_property(&label, id, "_partition"),
            Value::String("a".into())
        );
    }

    #[test]
    fn managed_records_preserve_identity_properties_labels_and_allocator() {
        let host = host();
        let store = ManagedStore::new("managed_graph");
        store.create(host.as_ref()).unwrap();
        let graph=import_graph(&serde_json::json!({"nodes":[{"id":42,"id_type":"Long","label":"person","properties":{},"property_records":[{"id":91,"id_type":"Long","key":"name","value":"one","meta":{"since":2020},"meta_types":{"since":"Integer"}},{"id":92,"id_type":"Long","key":"name","value":"two","meta":{}}]},{"id":"other","label":"other","properties":{"age":3}}],"edges":[{"id":7,"id_type":"Long","label":"knows","src":42,"dst":"other","properties":{"weight":0.5}}]})).unwrap();
        let person = graph
            .find_element_by_public_id(&Value::Long(42), false)
            .unwrap();
        graph
            .set_node_labels(&person, vec!["person".into(), "Employee".into()])
            .unwrap();
        store.persist(host.as_ref(), &graph).unwrap();
        let graph = store.attach(host.clone()).unwrap();
        let person = graph
            .find_element_by_public_id(&Value::Long(42), false)
            .unwrap();
        let Value::Node { label, id } = &person else {
            panic!()
        };
        assert_eq!(
            graph.node_labels(label, id.clone()),
            vec!["Employee", "person"]
        );
        let props = graph.properties(&person, &["name".into()]);
        assert_eq!(props.len(), 2);
        assert_eq!(graph.element_public_id(&props[0]), Value::Long(91));
        let meta = graph.properties(&props[0], &[]);
        assert_eq!(meta.len(), 1);
        let added = graph
            .set_vertex_property(
                &person,
                "name",
                Value::String("three".into()),
                Cardinality::List,
                BTreeMap::new(),
            )
            .unwrap();
        if let Value::VertexProperty { id, .. } = added {
            assert!(id >= 93);
        } else {
            panic!()
        }
        let snapshot = observe(&graph).unwrap();
        assert_eq!(snapshot.nodes.len(), 2);
        assert_eq!(snapshot.relationships.len(), 1);
        store.persist(host.as_ref(), &graph).unwrap();
        let restored = store.attach(host).unwrap();
        let person = restored
            .find_element_by_public_id(&Value::Long(42), false)
            .unwrap();
        assert_eq!(restored.properties(&person, &["name".into()]).len(), 3);
    }
    #[test]
    fn managed_updates_use_caller_transaction_and_retain_other_properties() {
        let host = host();
        let store = ManagedStore::new("managed_graph");
        store.create(host.as_ref()).unwrap();
        let graph = PropertyGraph::new();
        let node = graph.insert_node(
            "A",
            BTreeMap::from([
                ("x".into(), Value::Int(1)),
                ("y".into(), Value::String("keep".into())),
            ]),
        );
        store.persist(host.as_ref(), &graph).unwrap();
        host.execute(HostRequest::new("BEGIN TRANSACTION")).unwrap();
        let graph = store.attach(host.clone()).unwrap();
        graph.set_property(&node, "x", Value::Int(2)).unwrap();
        store.persist(host.as_ref(), &graph).unwrap();
        let changed = store.attach(host.clone()).unwrap();
        assert_eq!(changed.node_property("A", 0.into(), "x"), Value::Int(2));
        assert_eq!(
            changed.node_property("A", 0.into(), "y"),
            Value::String("keep".into())
        );
        host.execute(HostRequest::new("ROLLBACK")).unwrap();
        let restored = store.attach(host.clone()).unwrap();
        assert_eq!(restored.node_property("A", 0.into(), "x"), Value::Int(1));
        let b = restored.insert_node("B", BTreeMap::new());
        let c = restored.insert_node("C", BTreeMap::new());
        restored
            .insert_edge("R", &node, &b, BTreeMap::new())
            .unwrap();
        restored.insert_edge("R", &b, &c, BTreeMap::new()).unwrap();
        store.persist(host.as_ref(), &restored).unwrap();
        let restored = store.attach(host.clone()).unwrap();
        assert_eq!(restored.edge_ids("R").len(), 2);
        let d = restored.insert_node("D", BTreeMap::new());
        restored.insert_edge("R", &c, &d, BTreeMap::new()).unwrap();
        store.persist(host.as_ref(), &restored).unwrap();
        assert_eq!(store.attach(host).unwrap().edge_ids("R").len(), 3);
    }
