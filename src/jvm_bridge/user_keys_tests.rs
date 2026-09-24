//! Provider keys are user data, not native Arrow field access.
use super::*;
use crate::ir::catalog::{edges_from_columns, nodes_from_columns};
use arrow::array::Int64Array;
use std::sync::Arc;

fn call(store: &mut Store, request: Json) -> Json {
    let response = store.request(&request);
    assert_eq!(response["ok"], true, "{request}: {response}");
    response["value"].clone()
}

fn properties(store: &mut Store, owner: &Json, keys: &[&str]) -> Json {
    call(
        store,
        json!({"op":"properties","owner":owner["handle"],"keys":keys}),
    )
}

#[test]
fn double_underscore_keys_preserve_cardinality_identity_and_property_handles() {
    let mut store = Store::new();
    let vertex = call(
        &mut store,
        json!({"op":"addVertex","label":"__account",
        "id":{"type":"string","value":"native-id"},
        "properties":[["__id",{"type":"string","value":"strategy-id"}]]}),
    );
    let first = properties(&mut store, &vertex, &["__id"])[0].clone();
    let second = call(
        &mut store,
        json!({"op":"setVertexProperty","owner":vertex["handle"],
        "key":"__id","cardinality":"list","id":{"type":"string","value":"property-id"},
        "value":{"type":"string","value":"second"},
        "meta":[["__source",{"type":"string","value":"explicit"}]]}),
    );
    assert_ne!(first["id"], second["id"]);
    assert_eq!(second["id"], json!({"type":"string","value":"property-id"}));
    assert_eq!(
        properties(&mut store, &vertex, &["__id"]),
        json!([first, second])
    );
    assert_eq!(properties(&mut store, &vertex, &[]), json!([first, second]));
    assert_eq!(
        properties(&mut store, &second, &["__source"])[0]["value"]["value"],
        "explicit"
    );
    assert!(store.resolve(&second["handle"]).is_ok());
    let reference = json!({"type":"vertex_property_ref","owner":{"type":"vertex_ref",
        "id":vertex["id"]},"id":second["id"]});
    assert_eq!(
        store.encode(&store.decode(&reference).unwrap()).unwrap(),
        second
    );
    assert_eq!(
        call(&mut store, json!({"op":"vertices","ids":[vertex["id"]]})),
        json!([vertex])
    );
    assert_eq!(
        call(
            &mut store,
            json!({"op":"vertices","ids":[{"type":"string","value":"strategy-id"}]})
        ),
        json!([])
    );

    let duplicate = call(
        &mut store,
        json!({"op":"setVertexProperty","owner":vertex["handle"],
        "key":"__id","cardinality":"set","value":{"type":"string","value":"second"}}),
    );
    assert_eq!(duplicate["id"], second["id"]);
    assert_eq!(
        properties(&mut store, &vertex, &["__id"])
            .as_array()
            .unwrap()
            .len(),
        2
    );
    let replacement = call(
        &mut store,
        json!({"op":"setVertexProperty","owner":vertex["handle"],
        "key":"__id","cardinality":"single","value":{"type":"null"}}),
    );
    assert_eq!(
        properties(&mut store, &vertex, &["__id"]),
        json!([replacement])
    );
    assert!(store.resolve(&second["handle"]).is_err());
    call(
        &mut store,
        json!({"op":"remove","owner":replacement["handle"]}),
    );
    assert_eq!(properties(&mut store, &vertex, &["__id"]), json!([]));
}

#[test]
fn arrow_reserved_columns_are_never_materialized_as_jvm_user_properties() {
    let mut graph = PropertyGraph::new();
    graph.add_nodes(nodes_from_columns(
        "node",
        vec![("__private", Arc::new(Int64Array::from(vec![99, 100])))],
    ));
    graph
        .add_edges(edges_from_columns(
            "link",
            "node",
            "node",
            vec![0],
            vec![1],
            vec![],
        ))
        .unwrap();
    let mut store = Store::from_graph(graph);
    let vertices = call(&mut store, json!({"op":"vertices"}));
    let vertex = vertices[0].clone();
    let edge = call(&mut store, json!({"op":"edges"}))[0].clone();
    assert_eq!(properties(&mut store, &vertex, &[]), json!([]));
    assert_eq!(properties(&mut store, &vertex, &["__private"]), json!([]));
    assert_eq!(
        properties(&mut store, &edge, &["__src_id", "__dst_id"]),
        json!([])
    );
    assert_eq!(properties(&mut store, &edge, &[]), json!([]));

    for (i, cardinality) in ["list", "set"].into_iter().enumerate() {
        let owner = &vertices[i];
        let property = call(
            &mut store,
            json!({"op":"setVertexProperty","owner":owner["handle"],
            "key":"__private","cardinality":cardinality,"value":{"type":"int","value":7}}),
        );
        assert_eq!(
            properties(&mut store, owner, &["__private"]),
            json!([property])
        );
        assert!(
            store
                .graph
                .properties(
                    &store.resolve(&owner["handle"]).unwrap(),
                    &["__private".into()]
                )
                .is_empty()
        );
    }
    for key in ["__src_id", "__dst_id", "__custom"] {
        let property = call(
            &mut store,
            json!({"op":"setProperty","owner":edge["handle"],
            "key":key,"value":{"type":"string","value":"user-value"}}),
        );
        assert_eq!(properties(&mut store, &edge, &[key]), json!([property]));
        assert!(store.resolve(&property["handle"]).is_ok());
        assert_eq!(call(&mut store, json!({"op":"edges"})), json!([edge]));
        assert_eq!(
            call(
                &mut store,
                json!({"op":"adjacent","vertex":vertex["handle"],"direction":"OUT"})
            ),
            json!([edge])
        );
        assert!(
            store
                .graph
                .properties(&store.resolve(&edge["handle"]).unwrap(), &[key.into()])
                .is_empty()
        );
        call(
            &mut store,
            json!({"op":"remove","owner":property["handle"]}),
        );
        assert_eq!(properties(&mut store, &edge, &[key]), json!([]));
    }
    let null_property = call(
        &mut store,
        json!({"op":"setProperty","owner":edge["handle"],
        "key":"__src_id","value":{"type":"null"}}),
    );
    assert_eq!(properties(&mut store, &edge, &["__src_id"]), json!([null_property]));
    assert_eq!(
        store.graph.edge_endpoints("link", 0),
        Some(("node".into(), 0, "node".into(), 1))
    );
}

#[test]
fn user_keys_survive_persistence_and_rollback_without_exposing_native_columns() {
    let path = std::env::temp_dir().join(format!(
        "crabgraph-jvm-keys-{}-{}.ngsp",
        std::process::id(),
        SESSION_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let mut store = Store::open(&path).unwrap();
    let vertex = call(
        &mut store,
        json!({"op":"addVertex","properties":[["__custom",{"type":"int","value":1}]]}),
    );
    let vp = call(
        &mut store,
        json!({"op":"setVertexProperty","owner":vertex["handle"],
        "key":"__custom","cardinality":"list","value":{"type":"int","value":2},
        "meta":[["__meta",{"type":"string","value":"kept"}]]}),
    );
    let edge = call(
        &mut store,
        json!({"op":"addEdge","out":vertex["handle"],"in":vertex["handle"],
        "label":"__link","properties":[["__id",{"type":"string","value":"edge-user-id"}]]}),
    );
    call(&mut store, json!({"op":"commit"}));
    call(&mut store, json!({"op":"remove","owner":vp["handle"]}));
    let ep = properties(&mut store, &edge, &["__id"])[0].clone();
    call(&mut store, json!({"op":"remove","owner":ep["handle"]}));
    call(&mut store, json!({"op":"rollback"}));
    assert_eq!(
        properties(&mut store, &vertex, &["__custom"])
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        properties(&mut store, &edge, &["__id"])[0]["value"]["value"],
        "edge-user-id"
    );
    drop(store);

    let mut reopened = Store::open(&path).unwrap();
    let restored_vertex = call(&mut reopened, json!({"op":"vertices"}))[0].clone();
    let restored_edge = call(&mut reopened, json!({"op":"edges"}))[0].clone();
    assert_eq!(restored_vertex["id"], vertex["id"]);
    assert_eq!(restored_edge["id"], edge["id"]);
    let records = properties(&mut reopened, &restored_vertex, &["__custom"]);
    assert_eq!(records.as_array().unwrap().len(), 2);
    assert_eq!(records[1]["id"], vp["id"]);
    assert_eq!(
        properties(&mut reopened, &records[1], &["__meta"])[0]["value"]["value"],
        "kept"
    );
    assert_eq!(
        properties(&mut reopened, &restored_edge, &["__id"])[0]["value"]["value"],
        "edge-user-id"
    );
    assert!(
        reopened
            .graph
            .properties(&reopened.resolve(&restored_vertex["handle"]).unwrap(), &[])
            .is_empty()
    );
    assert!(
        reopened
            .graph
            .properties(&reopened.resolve(&restored_edge["handle"]).unwrap(), &[])
            .is_empty()
    );
    drop(reopened);
    std::fs::remove_file(&path).unwrap();
    std::fs::remove_file(path.with_extension("ngsp.lock")).unwrap();
}

#[test]
fn jvm_names_still_reject_empty_and_hidden_names() {
    for name in ["", "~hidden"] {
        let mut store = Store::new();
        assert_eq!(
            store.request(&json!({"op":"addVertex","label":name}))["ok"],
            false
        );
        let vertex = call(&mut store, json!({"op":"addVertex"}));
        assert_eq!(
            store.request(&json!({"op":"setVertexProperty","owner":vertex["handle"],
            "key":name,"cardinality":"single","value":{"type":"int","value":1}}))["ok"],
            false
        );
        assert_eq!(
            store
                .request(&json!({"op":"addVertex","properties":[[name,{"type":"int","value":1}]]}))
                ["ok"],
            false
        );
        assert_eq!(properties(&mut store, &vertex, &[]), json!([]));
    }
}
