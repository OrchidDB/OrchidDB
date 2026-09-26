#[path = "common/execution.rs"]
mod datafusion_test;
use orchiddb::ir::catalog::{Cardinality, PropertyGraph};
use crate::datafusion_test::execute_rows;
use orchiddb::ir::value::Value;
use orchiddb::language::gremlin::{GremlinPlanner, parse_traversal};
use orchiddb::storage::{decode_graph, encode_graph};
use std::collections::BTreeMap;
fn values(g: &PropertyGraph, q: &str) -> Vec<Value> {
    let plan = GremlinPlanner::new()
        .plan(&parse_traversal(q).unwrap())
        .unwrap();
    execute_rows(&plan, g)
        .unwrap()
        .into_iter()
        .map(|r| r.bindings["current"].clone())
        .collect()
}
fn id(p: &Value) -> i64 {
    if let Value::VertexProperty { id, .. } = p {
        *id
    } else {
        panic!("not vertex property: {p:?}")
    }
}
#[test]
fn list_scalar_and_multi_records_are_distinct_and_stable() {
    let g = PropertyGraph::new();
    let v = g.insert_node("unusual", BTreeMap::new());
    let p = g
        .set_vertex_property(
            &v,
            "tags",
            Value::List(vec![Value::Int(1), Value::Int(2)]),
            Cardinality::List,
            BTreeMap::new(),
        )
        .unwrap();
    let q = g
        .set_vertex_property(
            &v,
            "tags",
            Value::Int(3),
            Cardinality::List,
            BTreeMap::new(),
        )
        .unwrap();
    assert_ne!(id(&p), id(&q));
    assert_eq!(g.properties(&v, &["tags".into()]).len(), 2);
    assert_eq!(
        values(&g, "g.V().values('tags')"),
        vec![
            Value::List(vec![Value::Int(1), Value::Int(2)]),
            Value::Int(3)
        ]
    );
    let set = g
        .set_vertex_property(&v, "tags", Value::Int(3), Cardinality::Set, BTreeMap::new())
        .unwrap();
    assert_eq!(id(&set), id(&q));
    let empty_map = g
        .set_vertex_property(
            &v,
            "empty",
            Value::Map(BTreeMap::new()),
            Cardinality::Single,
            BTreeMap::new(),
        )
        .unwrap();
    let restored = decode_graph(&encode_graph(&g).unwrap()).unwrap();
    assert_eq!(restored.properties(&v, &[]), g.properties(&v, &[]));
    let r = restored
        .set_vertex_property(
            &v,
            "tags",
            Value::Int(4),
            Cardinality::Single,
            BTreeMap::new(),
        )
        .unwrap();
    assert!(id(&r) > id(&q));
    assert_eq!(restored.properties(&v, &["tags".into()]), vec![r]);
    assert_eq!(restored.properties(&v, &["empty".into()]), vec![empty_map]);
}
#[test]
fn meta_properties_drop_only_targeted_records() {
    let g = PropertyGraph::new();
    values(
        &g,
        "g.addV('citizen').property(list,'place','A','since',2000).property(list,'place','B','since',2010)",
    );
    assert_eq!(
        values(&g, "g.V().properties('place').values('since')"),
        vec![Value::Int(2000), Value::Int(2010)]
    );
    values(
        &g,
        "g.V().properties('place').hasValue('A').properties('since').drop()",
    );
    assert_eq!(
        values(&g, "g.V().properties('place').values('since')"),
        vec![Value::Int(2010)]
    );
    values(&g, "g.V().properties('place').hasValue('B').drop()");
    assert_eq!(
        values(&g, "g.V().values('place')"),
        vec![Value::String("A".into())]
    );
    assert_eq!(values(&g, "g.V().count()"), vec![Value::Long(1)]);
}
#[test]
fn public_identity_survives_snapshot_and_has_checks_every_record() {
    let g = PropertyGraph::new();
    values(
        &g,
        "g.addV('different').property(T.id,'hello').property(list,'age',1).property(list,'age',2)",
    );
    assert_eq!(
        values(&g, "g.V('hello').has('age',2).id()"),
        vec![Value::String("hello".into())]
    );
    assert_eq!(
        values(&g, "g.V('hello').asString()"),
        vec![Value::String("v[hello]".into())]
    );
    let restored = decode_graph(&encode_graph(&g).unwrap()).unwrap();
    assert_eq!(
        values(&restored, "g.V('hello').values('age')"),
        vec![Value::Int(1), Value::Int(2)]
    );
    let p = restored.properties(
        &restored
            .find_element_by_public_id(&Value::String("hello".into()), false)
            .unwrap(),
        &[],
    )[0]
    .clone();
    restored
        .set_vertex_property_public_id(&p, Value::String("prop-id".into()))
        .unwrap();
    let again = decode_graph(&encode_graph(&restored).unwrap()).unwrap();
    assert_eq!(again.element_public_id(&p), Value::String("prop-id".into()));
}
#[test]
fn edge_and_meta_property_owners_remain_native() {
    let g = PropertyGraph::new();
    let a = g.insert_node("x", BTreeMap::new());
    let b = g.insert_node("y", BTreeMap::new());
    let e = g
        .insert_edge("route", &a, &b, [("weight".into(), Value::Int(9))].into())
        .unwrap();
    let props = g.properties(&e, &[]);
    assert!(matches!(&props[0],Value::Property{owner,..}if owner.as_ref()==&e));
    g.delete_value(&props[0], true).unwrap();
    assert_eq!(g.edge_ids("route").len(), 1);
    assert!(g.properties(&e, &[]).is_empty());
}
#[test]
fn ordinary_property_shaped_maps_are_never_properties() {
    let g = PropertyGraph::new();
    assert_eq!(
        values(
            &g,
            "g.inject(['element':1,'key':'name','value':'plain']).element()"
        ),
        Vec::<Value>::new()
    );
}

#[test]
fn property_map_cardinality_defaults_and_per_key_overrides() {
    let graph = PropertyGraph::new();
    values(
        &graph,
        "g.addV().property(single,'name','foo').property('age',42)",
    );
    values(
        &graph,
        "g.V().property(['name':Cardinality.set('bar'),'age':43])",
    );
    assert_eq!(
        values(&graph, "g.V().values('name')"),
        vec![Value::String("foo".into()), Value::String("bar".into())]
    );
    assert_eq!(values(&graph, "g.V().values('age')"), vec![Value::Int(43)]);
    values(
        &graph,
        "g.V().property(set,['name':'baz','age':Cardinality.single(44)])",
    );
    assert_eq!(
        values(&graph, "g.V().values('name').count()"),
        vec![Value::Long(3)]
    );
    assert_eq!(values(&graph, "g.V().values('age')"), vec![Value::Int(44)]);
}

#[test]
fn imported_property_ids_reserve_generated_ids_and_typed_set_members_stay_distinct() {
    let graph = PropertyGraph::new();
    let v = graph.insert_node("x", BTreeMap::new());
    let first = graph
        .set_vertex_property(&v, "x", Value::Int(1), Cardinality::Set, BTreeMap::new())
        .unwrap();
    graph
        .set_vertex_property_public_id(&first, Value::Long(1000))
        .unwrap();
    let second = graph
        .set_vertex_property(&v, "x", Value::Long(1), Cardinality::Set, BTreeMap::new())
        .unwrap();
    assert_eq!(graph.properties(&v, &[]).len(), 2);
    assert!(graph.element_public_id(&second).as_i64().unwrap() > 1000);
}
#[test]
fn ordinary_property_equality_compares_key_and_value_across_owners() {
    let graph = PropertyGraph::new();
    let a = graph.insert_node("x", BTreeMap::new());
    let b = graph.insert_node("y", BTreeMap::new());
    for (src, dst) in [(&a, &b), (&b, &a)] {
        graph
            .insert_edge("route", src, dst, [("weight".into(), Value::Int(9))].into())
            .unwrap();
    }
    assert_eq!(
        values(&graph, "g.E().properties('weight').dedup().count()"),
        vec![Value::Long(1)]
    );
}
#[cfg(feature = "duckdb")]
#[tokio::test]
async fn scalar_element_strings_keep_public_ids_across_sql_boundaries() {
    let mut engine = orchiddb::engine::GraphEngine::in_memory().unwrap();
    engine.gremlin("g.addV('x').property(T.id,'left').as('a').addV('y').property(T.id,'right').addE('route').from('a').property(T.id,'link')").await.unwrap();
    let vertices = engine.gremlin("g.V().asString()").await.unwrap();
    let rows: serde_json::Value = serde_json::from_str(
        &vertices.returned.batch.schema().metadata()["orchiddb.gremlin.typed_rows.v1"],
    )
    .unwrap();
    assert_eq!(rows[0][0]["value"], "v[left]");
    assert_eq!(rows[1][0]["value"], "v[right]");
    let edges = engine.gremlin("g.E().asString()").await.unwrap();
    let rows: serde_json::Value = serde_json::from_str(
        &edges.returned.batch.schema().metadata()["orchiddb.gremlin.typed_rows.v1"],
    )
    .unwrap();
    assert_eq!(rows[0][0]["value"], "e[link][left-route->right]");
}

#[test]
fn vertex_property_strategy_evaluates_native_metadata_without_affecting_edge_properties() {
    let graph = PropertyGraph::new();
    values(
        &graph,
        "g.addV('sensor').property(list,'reading',7,'approved',true).property(list,'reading',9,'approved',false).as('a').addV('sensor').property(list,'reading',11,'approved',true).addE('link').from('a').property('reading',13)",
    );
    let strategy =
        "g.withStrategies(new SubgraphStrategy(vertexProperties: __.has('approved',true)))";
    assert_eq!(
        values(&graph, &format!("{strategy}.V().values('reading')")),
        vec![Value::Int(7), Value::Int(11)]
    );
    assert_eq!(
        values(
            &graph,
            &format!("{strategy}.V().properties('reading').value()")
        ),
        vec![Value::Int(7), Value::Int(11)]
    );
    assert_eq!(
        values(
            &graph,
            &format!("{strategy}.E().properties('reading').value()")
        ),
        vec![Value::Int(13)]
    );
    assert_eq!(
        values(
            &graph,
            &format!("{strategy}.V().properties('reading').properties('approved').value()")
        ),
        vec![Value::Bool(true), Value::Bool(true)]
    );
}

#[test]
fn null_string_cast_and_repeat_predicates_preserve_public_identity() {
    let graph = PropertyGraph::new();
    assert_eq!(values(&graph, "g.inject(null,1).asString()"), vec![Value::Null, Value::String("1".into())]);
    values(&graph, "g.addV('x').property(T.id,100).as('a').addV('x').property(T.id,200).addE('link').from('a')");
    assert_eq!(values(&graph, "g.V(100).repeat(out()).until(hasId(200)).id()"), vec![Value::Int(200)]);
}

#[test]
fn set_cardinality_uses_typed_nan_and_decimal_identity_after_snapshot() {
    let graph = PropertyGraph::new();
    let vertex = graph.insert_node("measure", BTreeMap::new());
    let nan = graph.set_vertex_property(&vertex, "sample", Value::Float(f64::from_bits(0x7ff8_0000_0000_0001)), Cardinality::Set, BTreeMap::new()).unwrap();
    let duplicate_nan = graph.set_vertex_property(&vertex, "sample", Value::Float(f64::from_bits(0x7ff8_0000_0000_0002)), Cardinality::Set, BTreeMap::new()).unwrap();
    assert_eq!(id(&nan), id(&duplicate_nan));
    for raw in ["1.0", "1.00", "1.0"] {
        graph.set_vertex_property(&vertex, "sample", Value::BigDecimal(raw.parse().unwrap()), Cardinality::Set, BTreeMap::new()).unwrap();
    }
    assert_eq!(graph.properties(&vertex, &["sample".into()]).len(), 3);
    let set = Value::Set(vec![Value::Int(1), Value::Long(1)]);
    let nested = graph.set_vertex_property(&vertex, "collection", set.clone(), Cardinality::Set, BTreeMap::new()).unwrap();
    let restored = decode_graph(&encode_graph(&graph).unwrap()).unwrap();
    assert_eq!(values(&restored, "g.V().values('collection')"), vec![set]);
    let duplicate_nested = restored.set_vertex_property(&vertex, "collection", Value::Set(vec![Value::Long(1), Value::Int(1)]), Cardinality::Set, BTreeMap::new()).unwrap();
    assert_eq!(id(&nested), id(&duplicate_nested));
    let restored_nan = restored.set_vertex_property(&vertex, "sample", Value::Float(f64::NAN), Cardinality::Set, BTreeMap::new()).unwrap();
    assert_eq!(id(&nan), id(&restored_nan));
    assert_eq!(restored.properties(&vertex, &["sample".into()]).len(), 3);
}

#[test]
fn native_property_equality_and_dedup_use_the_same_typed_value_identity() {
    let graph = PropertyGraph::new();
    let a = graph.insert_node("a", BTreeMap::new());
    let b = graph.insert_node("b", BTreeMap::new());
    for (src, dst, bits) in [(&a, &b, 0x7ff8_0000_0000_0001), (&b, &a, 0x7ff8_0000_0000_0002)] {
        graph.insert_edge("link", src, dst, [("reading".into(), Value::Float(f64::from_bits(bits)))].into()).unwrap();
    }
    let properties = values(&graph, "g.E().properties('reading')");
    assert_eq!(properties[0], properties[1]);
    assert_eq!(properties[0].three_valued_eq(&properties[1]), Some(true));
    assert_eq!(values(&graph, "g.E().properties('reading').dedup().count()"), vec![Value::Long(1)]);
    let native_set = orchiddb::ir::value::gremlin_set(properties);
    assert!(matches!(native_set, Value::Set(items) if items.len() == 1));
}
