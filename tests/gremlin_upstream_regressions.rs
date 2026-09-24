//! Engine regressions identified by the upstream TinkerPop scenarios.
use new_graph::ir::catalog::PropertyGraph;
use new_graph::ir::interpreter::execute_rows;
use new_graph::ir::value::Value;
use new_graph::language::gremlin::{GremlinPlanner, parse_traversal};

fn values(query: &str) -> Vec<Value> {
    let traversal = parse_traversal(query).unwrap();
    let plan = GremlinPlanner::new().plan(&traversal).unwrap();
    execute_rows(&plan, &PropertyGraph::new())
        .unwrap()
        .into_iter()
        .map(|row| row.bindings["current"].clone())
        .collect()
}

#[test]
fn choose_options_drop_unmatched_traversers() {
    assert_eq!(
        values("g.inject(1,2,3).choose(__.identity()).option(2,__.constant('matched'))"),
        vec![Value::String("matched".into())]
    );
}

#[test]
fn choose_options_apply_explicit_none_fallback() {
    assert_eq!(
        values(
            "g.inject(1,2,3).choose(__.identity()).option(2,__.constant('matched')).option(Pick.none,__.constant('fallback'))"
        ),
        vec![
            Value::String("fallback".into()),
            Value::String("matched".into()),
            Value::String("fallback".into())
        ]
    );
}

fn graph_values(query: &str, graph: &PropertyGraph) -> Vec<Value> {
    let plan = GremlinPlanner::new()
        .plan(&parse_traversal(query).unwrap())
        .unwrap();
    execute_rows(&plan, graph)
        .unwrap()
        .into_iter()
        .map(|row| row.bindings["current"].clone())
        .collect()
}

#[test]
fn vertex_ids_are_unique_across_labels_and_roundtrip() {
    let graph = PropertyGraph::new();
    graph.insert_node("person", Default::default());
    graph.insert_node("software", Default::default());
    assert_eq!(
        graph_values("g.V().id()", &graph),
        vec![
            Value::String("person#0".into()),
            Value::String("software#0".into())
        ]
    );
    assert_eq!(
        graph_values("g.V('software#0').label()", &graph),
        vec![Value::String("software".into())]
    );
}

#[test]
fn explicit_vertex_ids_roundtrip_through_source_lookup() {
    let graph = PropertyGraph::new();
    graph.insert_node("person", [("id".into(), Value::Int(42))].into());
    assert_eq!(graph_values("g.V().id()", &graph), vec![Value::Int(42)]);
    assert_eq!(
        graph_values("g.V(42).label()", &graph),
        vec![Value::String("person".into())]
    );
    assert!(graph_values("g.V(0)", &graph).is_empty());
    assert_eq!(
        graph_values("g.V().hasId(P.within(42)).id()", &graph),
        vec![Value::Int(42)]
    );
}

#[test]
fn edge_ids_are_unique_across_labels_and_roundtrip() {
    let graph = PropertyGraph::new();
    let a = graph.insert_node("person", Default::default());
    let b = graph.insert_node("software", Default::default());
    graph
        .insert_edge("created", &a, &b, Default::default())
        .unwrap();
    graph
        .insert_edge("uses", &a, &b, Default::default())
        .unwrap();
    assert_eq!(
        graph_values("g.E().id()", &graph),
        vec![
            Value::String("created#0".into()),
            Value::String("uses#0".into())
        ]
    );
    assert_eq!(
        graph_values("g.E('uses#0').label()", &graph),
        vec![Value::String("uses".into())]
    );
}

#[test]
fn value_maps_preserve_actual_numeric_and_list_types() {
    let graph = PropertyGraph::new();
    let a = graph.insert_node("person", [("age".into(), Value::Int(29))].into());
    let b = graph.insert_node("software", Default::default());
    graph
        .insert_edge(
            "created",
            &a,
            &b,
            [("weight".into(), Value::Float(0.5))].into(),
        )
        .unwrap();
    let Value::Map(vertex) = &graph_values("g.V().hasLabel('person').valueMap()", &graph)[0] else {
        panic!("expected map")
    };
    assert_eq!(vertex["age"], Value::List(vec![Value::Int(29)]));
    let Value::Map(edge) = &graph_values("g.E().valueMap()", &graph)[0] else {
        panic!("expected map")
    };
    assert_eq!(edge["weight"], Value::Float(0.5));
    let Value::TypedMap(element) =
        &graph_values("g.V().hasLabel('person').elementMap()", &graph)[0]
    else {
        panic!("expected typed map")
    };
    assert!(element.contains(&(Value::String("age".into()), Value::Int(29))));
    assert!(element.contains(&(Value::Token("id".into()), Value::String("person#0".into()))));
}

#[test]
fn id_projection_modulator_returns_actual_identity() {
    let graph = PropertyGraph::new();
    graph.insert_node("person", Default::default());
    let Value::Map(map) = &graph_values("g.V().project('identity').by(T.id)", &graph)[0] else {
        panic!("expected map")
    };
    assert_eq!(map["identity"], Value::String("person#0".into()));
}

#[test]
fn group_counts_are_long_values_for_direct_and_side_effect_maps() {
    for query in [
        "g.inject('a','b','a').groupCount()",
        "g.inject('a','b','a').groupCount('counts').cap('counts')",
    ] {
        let results = values(query);
        let Value::Map(map) = &results[0] else {
            panic!("expected map")
        };
        assert_eq!(map["a"], Value::Long(2), "{query}");
        assert_eq!(map["b"], Value::Long(1), "{query}");
    }
}

#[test]
fn numeric_literal_suffixes_keep_their_tinkerpop_types() {
    assert_eq!(
        values("g.inject(1B,2S,3I,4L,5.25F,6.5D,7N,8.125M)"),
        vec![
            Value::Byte(1),
            Value::Short(2),
            Value::Int(3),
            Value::Long(4),
            Value::Float32(5.25),
            Value::Float(6.5),
            Value::BigInt(7.into()),
            Value::BigDecimal("8.125".parse().unwrap()),
        ]
    );
    assert_eq!(values("g.inject(0).constant(4L)"), vec![Value::Long(4)]);
    assert_eq!(
        values("g.inject(0).constant([1B,2S,4L,5.25F])"),
        vec![Value::List(vec![
            Value::Byte(1),
            Value::Short(2),
            Value::Long(4),
            Value::Float32(5.25),
        ])]
    );
}

#[test]
fn numeric_literal_boundaries_are_lossless() {
    assert_eq!(
        values(
            "g.inject(-9223372036854775808L,2147483648,9223372036854775808N,9223372036854775808)"
        ),
        vec![
            Value::Long(i64::MIN),
            Value::Long(2147483648),
            Value::BigInt("9223372036854775808".parse().unwrap()),
            Value::BigInt("9223372036854775808".parse().unwrap()),
        ]
    );
    assert!(parse_traversal("g.inject(128B)").is_err());
    assert!(parse_traversal("g.inject(32768S)").is_err());
    assert!(parse_traversal("g.inject(2147483648I)").is_err());
}

#[cfg(feature = "duckdb")]
#[tokio::test]
async fn numeric_types_survive_engine_execution() {
    let mut engine = new_graph::engine::GraphEngine::in_memory().unwrap();
    for (literal, expected) in [
        ("2B", "byte"),
        ("2S", "short"),
        ("2I", "int"),
        ("2L", "long"),
        ("2.25F", "float"),
    ] {
        let result = engine
            .gremlin(&format!("g.inject(0).constant({literal})"))
            .await
            .unwrap();
        let schema = result.returned.batch.schema();
        let rows: serde_json::Value =
            serde_json::from_str(&schema.metadata()["crabgraph.gremlin.typed_rows.v1"]).unwrap();
        assert_eq!(rows[0][0]["type"], expected, "literal {literal}");
    }
}

#[cfg(feature = "duckdb")]
#[tokio::test]
async fn compound_constants_survive_engine_execution() {
    let mut engine = new_graph::engine::GraphEngine::in_memory().unwrap();
    for (literal, expected) in [
        ("[1I,'1',2L]", "list"),
        ("['a':2I]", "map"),
        ("9223372036854775808N", "bigint"),
        ("1.123456789M", "bigdecimal"),
    ] {
        let result = engine
            .gremlin(&format!("g.inject(0).constant({literal})"))
            .await
            .unwrap();
        let schema = result.returned.batch.schema();
        let rows: serde_json::Value =
            serde_json::from_str(&schema.metadata()["crabgraph.gremlin.typed_rows.v1"]).unwrap();
        assert_eq!(rows[0][0]["type"], expected, "literal {literal}");
        if expected == "bigdecimal" {
            assert_eq!(rows[0][0]["value"], "1.123456789");
        }
    }
}

#[cfg(feature = "duckdb")]
#[tokio::test]
async fn property_integer_types_survive_sql_scan() {
    let mut engine = new_graph::engine::GraphEngine::in_memory().unwrap();
    let graph = PropertyGraph::new();
    graph.insert_node(
        "person",
        [
            ("age".into(), Value::Int(29)),
            ("long_age".into(), Value::Long(29)),
        ]
        .into(),
    );
    engine.replace_graph(graph).unwrap();
    for (key, kind) in [("age", "int"), ("long_age", "long")] {
        let result = engine
            .gremlin(&format!("g.V().values('{key}')"))
            .await
            .unwrap();
        let schema = result.returned.batch.schema();
        let rows: serde_json::Value =
            serde_json::from_str(&schema.metadata()["crabgraph.gremlin.typed_rows.v1"]).unwrap();
        assert_eq!(rows[0][0]["type"], kind, "property {key}");
    }
}

#[test]
fn add_vertex_and_property_mutations_create_real_graph_data() {
    let graph = PropertyGraph::new();
    assert_eq!(
        graph_values(
            "g.addV('person').property('name','marko').property('age',29I).values('age')",
            &graph
        ),
        vec![Value::Int(29)]
    );
    assert_eq!(
        graph_values("g.V().values('name')", &graph),
        vec![Value::String("marko".into())]
    );
    assert_eq!(
        graph_values(
            "g.V().property(Cardinality.single,'age',30I).values('age')",
            &graph
        ),
        vec![Value::Int(30)]
    );
    graph_values("g.V().addV('copy')", &graph);
    assert_eq!(
        graph_values("g.V().hasLabel('copy').count()", &graph),
        vec![Value::Long(1)]
    );
}

#[test]
fn legacy_none_discards_results_after_real_writes() {
    let graph = PropertyGraph::new();
    assert!(graph_values("g.addV().property('text','x.none()').none  ()", &graph).is_empty());
    assert_eq!(
        graph_values("g.V().values('text')", &graph),
        vec![Value::String("x.none()".into())]
    );
    assert_eq!(
        values("g.inject([1,2]).none(P.eq(3))"),
        vec![Value::List(vec![Value::Int(1), Value::Int(2)])]
    );
}

#[cfg(feature = "duckdb")]
#[tokio::test]
async fn vertex_mutations_commit_through_engine() {
    let mut engine = new_graph::engine::GraphEngine::in_memory().unwrap();
    engine
        .gremlin("g.addV('person').property('name','marko').none()")
        .await
        .unwrap();
    let result = engine.gremlin("g.V().values('name')").await.unwrap();
    let schema = result.returned.batch.schema();
    let rows: serde_json::Value =
        serde_json::from_str(&schema.metadata()["crabgraph.gremlin.typed_rows.v1"]).unwrap();
    assert_eq!(rows[0][0]["value"], "marko");
}

#[test]
fn add_edge_uses_named_endpoints_and_real_properties() {
    let graph = PropertyGraph::new();
    graph_values(
        "g.addV('person').property('name','marko').as('m').addV('person').property('name','vadas').as('v').addE('knows').from('m').to('v').property('weight',0.5D).none()",
        &graph,
    );
    assert_eq!(
        graph_values(
            "g.V().has('name','marko').out('knows').values('name')",
            &graph
        ),
        vec![Value::String("vadas".into())]
    );
    assert_eq!(
        graph_values("g.E().values('weight')", &graph),
        vec![Value::Float(0.5)]
    );
}

#[test]
fn merge_vertex_static_maps_match_create_and_update_all_matches() {
    let graph = PropertyGraph::new();
    graph_values(
        "g.mergeV([(T.label):'person','name':'marko']).option(Merge.onCreate,['age':29]).none()",
        &graph,
    );
    assert_eq!(
        graph_values("g.mergeV(['name':'marko']).values('age')", &graph),
        vec![Value::Int(29)]
    );
    graph_values("g.addV('person').property('name','marko').none()", &graph);
    assert_eq!(
        graph_values(
            "g.inject(0).mergeV(['name':'marko']).option(Merge.onMatch,['age':30]).values('age')",
            &graph
        ),
        vec![Value::Int(30), Value::Int(30)]
    );
    assert_eq!(graph_values("g.V().count()", &graph), vec![Value::Long(2)]);
}

#[test]
fn merge_vertex_null_maps_follow_upstream_materialization() {
    let graph = PropertyGraph::new();
    assert_eq!(
        graph_values("g.mergeV(null).count()", &graph),
        vec![Value::Long(1)]
    );
    assert_eq!(
        graph_values(
            "g.mergeV(['name':'absent']).option(Merge.onCreate,null).values('name')",
            &graph
        ),
        vec![Value::String("absent".into())]
    );
    assert_eq!(
        graph_values("g.mergeV(null).option(Merge.onCreate,[:]).count()", &graph),
        vec![Value::Long(2)]
    );
    assert_eq!(
        graph_values("g.mergeV([:]).count()", &graph),
        vec![Value::Long(2)]
    );
    assert_eq!(
        graph_values("g.mergeV(['T.label':'ordinary']).values('T.label')", &graph),
        vec![Value::String("ordinary".into())]
    );
}

#[test]
fn qualified_java_enum_names_are_lexical_aliases() {
    assert_eq!(
        values("g.inject([1,2]).count(Scope.local)"),
        vec![Value::Long(2)]
    );
    let graph = PropertyGraph::new();
    assert_eq!(
        graph_values(
            "g.addV().property(VertexProperty.Cardinality.single,'name','Scope.global').values('name')",
            &graph
        ),
        vec![Value::String("Scope.global".into())]
    );
    assert_eq!(
        graph_values(
            "g.addV().property('age',__.constant(2)).values('age')",
            &graph
        ),
        vec![Value::Int(2)]
    );
}

#[cfg(feature = "duckdb")]
#[tokio::test]
async fn merge_vertex_commits_and_matches_without_duplicate_creation() {
    let mut engine = new_graph::engine::GraphEngine::in_memory().unwrap();
    engine.gremlin("g.mergeV([(T.label):'person','name':'marko']).option(Merge.onCreate,['age':29]).none()").await.unwrap();
    engine
        .gremlin("g.mergeV(['name':'marko']).option(Merge.onMatch,['age':30]).none()")
        .await
        .unwrap();
    let result = engine.gremlin("g.V().values('age')").await.unwrap();
    let schema = result.returned.batch.schema();
    let rows: serde_json::Value =
        serde_json::from_str(&schema.metadata()["crabgraph.gremlin.typed_rows.v1"]).unwrap();
    assert_eq!(rows.as_array().unwrap().len(), 1);
    assert_eq!(rows[0][0]["value"], 30);
}

#[cfg(feature = "duckdb")]
#[tokio::test]
async fn both_edge_and_vertex_steps_count_self_loops_in_both_directions() {
    let mut engine = new_graph::engine::GraphEngine::in_memory().unwrap();
    engine
        .gremlin("g.addV('person').as('a').addE('loop').to('a').none()")
        .await
        .unwrap();
    for step in ["bothE()", "both()"] {
        let result = engine
            .gremlin(&format!("g.V().{step}.count()"))
            .await
            .unwrap();
        let schema = result.returned.batch.schema();
        let rows: serde_json::Value =
            serde_json::from_str(&schema.metadata()["crabgraph.gremlin.typed_rows.v1"]).unwrap();
        assert_eq!(rows[0][0]["value"], 2, "{step}");
    }
}

#[test]
fn merge_vertex_validates_public_keys_and_creation_overrides() {
    for (query, message) in [
        (
            "g.mergeV(['~id':1])",
            "Property key can not be a hidden key: ~id",
        ),
        (
            "g.mergeV([(T.label):'~vertex'])",
            "Label can not be a hidden key: ~vertex",
        ),
        (
            "g.mergeV([:]).option(Merge.onCreate,['~label':'x'])",
            "Property key can not be a hidden key: ~label",
        ),
        (
            "g.mergeV(['name':'a']).option(Merge.onCreate,['name':'b'])",
            "option(onCreate) cannot override values from merge() argument",
        ),
    ] {
        let parsed = parse_traversal(query).unwrap();
        assert!(
            GremlinPlanner::new()
                .plan(&parsed)
                .unwrap_err()
                .to_string()
                .contains(message),
            "{query}"
        );
    }
}

#[test]
fn merge_vertex_single_cardinality_values_are_real_values() {
    let graph = PropertyGraph::new();
    assert_eq!(
        graph_values(
            "g.mergeV(['name':'alice']).option(Merge.onCreate,['age':VertexProperty.Cardinality.single(81I)]).values('age')",
            &graph
        ),
        vec![Value::Int(81)]
    );
    assert_eq!(
        graph_values(
            "g.mergeV(['name':'alice']).option(Merge.onMatch,['age':Cardinality.single(82I)]).values('age')",
            &graph
        ),
        vec![Value::Int(82)]
    );
    assert!(
        parse_traversal(
            "g.mergeV(['name':'alice']).option(Merge.onCreate,['age':Cardinality.list(81I)])"
        )
        .is_err()
    );
}

#[test]
fn traversal_valued_properties_preserve_target_and_outer_labels() {
    let graph = PropertyGraph::new();
    assert_eq!(
        graph_values(
            "g.addV('person').property('age',29).as('a').addV('animal').property('age',__.select('a').by('age')).values('age')",
            &graph
        ),
        vec![Value::Int(29)]
    );
    assert_eq!(
        graph_values(
            "g.withSideEffect('a','marko').addV().property('name',__.select('a')).values('name')",
            &graph
        ),
        vec![Value::String("marko".into())]
    );
    assert_eq!(
        graph_values(
            "g.V().hasLabel('animal').property('age',__.constant(30)).values('age')",
            &graph
        ),
        vec![Value::Int(30)]
    );
}

#[cfg(feature = "duckdb")]
#[tokio::test]
async fn drop_vertices_and_edges_performs_real_deletions() {
    let mut engine = new_graph::engine::GraphEngine::in_memory().unwrap();
    engine
        .gremlin("g.addV().as('a').addV().addE('link').to('a').none()")
        .await
        .unwrap();
    engine.gremlin("g.E().drop()").await.unwrap();
    let result = engine.gremlin("g.E().count()").await.unwrap();
    let rows: serde_json::Value = serde_json::from_str(
        &result.returned.batch.schema().metadata()["crabgraph.gremlin.typed_rows.v1"],
    )
    .unwrap();
    assert_eq!(rows[0][0]["value"], 0);
    engine.gremlin("g.V().drop()").await.unwrap();
    let result = engine.gremlin("g.V().count()").await.unwrap();
    let rows: serde_json::Value = serde_json::from_str(
        &result.returned.batch.schema().metadata()["crabgraph.gremlin.typed_rows.v1"],
    )
    .unwrap();
    assert_eq!(rows[0][0]["value"], 0);
}

#[test]
fn merge_edges_match_create_update_with_real_endpoint_ids() {
    let graph = PropertyGraph::new();
    graph_values(
        "g.addV('person').property('name','marko').addV('person').property('name','vadas').none()",
        &graph,
    );
    assert_eq!(
        graph_values(
            "g.mergeE([(T.label):'knows',(Direction.OUT):'person#0',(Direction.IN):'person#1']).option(Merge.onCreate,['created':'Y']).values('created')",
            &graph
        ),
        vec![Value::String("Y".into())]
    );
    assert_eq!(
        graph_values(
            "g.mergeE([(T.label):'knows',(Direction.OUT):'person#0',(Direction.IN):'person#1']).option(Merge.onMatch,['created':'N']).values('created')",
            &graph
        ),
        vec![Value::String("N".into())]
    );
    assert_eq!(
        graph_values(
            "g.V().has('name','marko').out('knows').values('name')",
            &graph
        ),
        vec![Value::String("vadas".into())]
    );
    assert_eq!(graph_values("g.E().count()", &graph), vec![Value::Long(1)]);
    assert_eq!(
        graph_values("g.mergeE([:]).count()", &graph),
        vec![Value::Long(1)]
    );
    assert_eq!(
        graph_values("g.V().mergeE(null).count()", &graph),
        vec![Value::Long(2)]
    );
}

#[cfg(feature = "duckdb")]
#[tokio::test]
async fn merge_edge_commits_real_graph_writes() {
    let mut engine = new_graph::engine::GraphEngine::in_memory().unwrap();
    engine
        .gremlin("g.addV('p').addV('p').none()")
        .await
        .unwrap();
    engine
        .gremlin("g.mergeE([(T.label):'link',(Direction.OUT):'p#0',(Direction.IN):'p#1']).none()")
        .await
        .unwrap();
    let result = engine.gremlin("g.E().count()").await.unwrap();
    let rows: serde_json::Value = serde_json::from_str(
        &result.returned.batch.schema().metadata()["crabgraph.gremlin.typed_rows.v1"],
    )
    .unwrap();
    assert_eq!(rows[0][0]["value"], 1);
}

#[test]
fn native_vertex_references_resolve_catalog_identity_without_string_spoofing() {
    let graph = PropertyGraph::new();
    graph.insert_node("person", [("id".into(),Value::String("custom-a".into())),("name".into(),Value::String("marko".into()))].into());
    graph.insert_node("person", [("id".into(),Value::String("custom-b".into())),("name".into(),Value::String("vadas".into()))].into());
    assert_eq!(graph_values("g.V(new Vertex('custom-a','vertex')).values('name')", &graph), vec![Value::String("marko".into())]);
    assert_eq!(graph_values("g.inject(new Vertex('custom-a','vertex')).values('name')", &graph), vec![Value::String("marko".into())]);
    assert_eq!(graph_values("g.withSideEffect('v',new Vertex('custom-b','vertex')).inject(1).select('v').values('name')", &graph), vec![Value::String("vadas".into())]);
    assert_eq!(graph_values("g.mergeE([(T.label):'knows',(Direction.OUT):new Vertex('custom-a','vertex'),(Direction.IN):new Vertex('custom-b','vertex')]).label()", &graph), vec![Value::String("knows".into())]);
    assert_eq!(graph_values("g.V(new Vertex('custom-a','vertex')).out('knows').values('name')", &graph), vec![Value::String("vadas".into())]);
    assert_eq!(graph_values("g.inject('new Vertex(1,vertex)')", &graph), vec![Value::String("new Vertex(1,vertex)".into())]);
}
