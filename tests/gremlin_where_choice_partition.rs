mod gremlin_case_runner;

use gremlin_case_runner::{compare, dataset, format, parse};
use new_graph::ir::{catalog::PropertyGraph, interpreter::execute};
use new_graph::language::gremlin::planner::GremlinPlanner;

fn assert_rows(graph: &PropertyGraph, query: &str, expected: &[&str]) {
    let traversal = parse::gremlin_with_case(query, "").unwrap();
    let plan = GremlinPlanner::new().plan(&traversal).unwrap();
    let result = execute(&plan, graph).unwrap_or_else(|e| panic!("{query}: {e}"));
    let actual = format::lines_from_batch(&result);
    let expected = expected.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
    if let compare::Verdict::Mismatch { reason } =
        compare::matches(&actual, &expected, false, "rows")
    {
        panic!("{query}: {reason}\nactual={actual:?}\nexpected={expected:?}");
    }
}

#[test]
fn choose_matches_numeric_count_options_and_preserves_parent() {
    let graph = dataset::modern_graph();
    assert_rows(
        &graph,
        "g.V().choose(__.out().count()).option(2L, __.values('name')).option(3L, __.values('age'))",
        &["29", "josh"],
    );
    assert_rows(
        &graph,
        "g.inject(0,1,2).choose(__.identity()).option(1L, __.constant('one')).option(Pick.none, __.constant('other'))",
        &["other", "one", "other"],
    );
    assert_rows(
        &graph,
        "g.inject(0,1,2).branch(__.identity()).option(1L, __.constant('one')).option(Pick.any, __.constant('any'))",
        &["one", "any", "any", "any"],
    );
    assert_rows(
        &graph,
        "g.inject(0,1,2).branch(__.identity()).option(__.is(P.gt(0)), __.identity())",
        &["1", "2"],
    );
}

#[test]
fn where_cycles_modulators_across_nested_predicate_and_scalar_child() {
    let graph = dataset::modern_graph();
    assert_rows(
        &graph,
        "g.V().as('a').outE('created').as('b').inV().as('c').in('created').as('d').where('a', P.lt('b').or(P.gt('c')).and(P.neq('d'))).by('age').by('weight').by(__.in('created').values('age').min()).select('a','c','d').by('name')",
        &[
            "m[{\"a\":\"josh\",\"c\":\"lop\",\"d\":\"marko\"}]",
            "m[{\"a\":\"josh\",\"c\":\"lop\",\"d\":\"peter\"}]",
            "m[{\"a\":\"peter\",\"c\":\"lop\",\"d\":\"marko\"}]",
            "m[{\"a\":\"peter\",\"c\":\"lop\",\"d\":\"josh\"}]",
        ],
    );
}

#[test]
fn where_drops_unproductive_operands_before_boolean_evaluation() {
    let graph = dataset::modern_graph();
    assert_rows(
        &graph,
        "g.V().has('name','marko').as('a').out('created').as('b').where('a',P.eq('a').or(P.eq('b'))).by('age').values('name')",
        &[],
    );
    assert_rows(
        &graph,
        "g.withStrategies(ProductiveByStrategy).V().has('name','lop').as('a').where('a',P.eq('a')).by('age').values('name')",
        &["lop"],
    );
    assert_rows(
        &graph,
        "g.withStrategies(ProductiveByStrategy).V().has('name','marko').as('a').out('created').as('b').where('a',P.neq('b')).by('age').values('name')",
        &["lop"],
    );
}

#[test]
fn partition_edges_do_not_require_visible_opposite_vertex() {
    let graph = dataset::build_with_initializer("empty", Some("g.addV('person').property('_partition','a').property('name','alice').as('a').addV('person').property('_partition','b').property('name','bob').as('b').addE('knows').from('a').to('b').property('_partition','a').property('weight',1.0d).addE('knows').from('b').to('a').property('_partition','b').property('weight',2.0d)")).unwrap();
    for (partition, weight, name) in [("a", "1.0", "alice"), ("b", "2.0", "bob")] {
        let source = format!(
            "g.withStrategies(new PartitionStrategy(partitionKey:'_partition',readPartitions:['{partition}']))"
        );
        assert_rows(
            &graph,
            &format!("{source}.V().bothE().values('weight')"),
            &[weight],
        );
        assert_rows(&graph, &format!("{source}.E().values('weight')"), &[weight]);
        assert_rows(&graph, &format!("{source}.V().values('name')"), &[name]);
        assert_rows(&graph, &format!("{source}.V().both().values('name')"), &[]);
        assert_rows(
            &graph,
            &format!("{source}.V().bothE().otherV().values('name')"),
            &[],
        );
    }
}

#[cfg(feature = "duckdb")]
#[tokio::test]
async fn mixed_choice_preserves_native_integer_and_string_types() {
    let mut engine = new_graph::engine::GraphEngine::in_memory().unwrap();
    engine.replace_graph(dataset::modern_graph()).unwrap();
    let result = engine.gremlin("g.V().choose(__.out().count()).option(2L,__.values('name')).option(3L,__.values('age'))").await.unwrap();
    let rows: serde_json::Value = serde_json::from_str(
        &result.returned.batch.schema().metadata()["crabgraph.gremlin.typed_rows.v1"],
    )
    .unwrap();
    assert_eq!(rows.as_array().unwrap().len(), 2, "{rows}");
    assert!(
        rows.as_array()
            .unwrap()
            .iter()
            .any(|r| r[0]["type"] == "string" && r[0]["value"] == "josh"),
        "{rows}"
    );
    assert!(
        rows.as_array()
            .unwrap()
            .iter()
            .any(|r| r[0]["type"] == "int" && r[0]["value"] == 29),
        "{rows}"
    );
}

#[cfg(feature = "duckdb")]
#[tokio::test]
async fn side_effect_executes_all_child_mutations_and_preserves_empty_parent() {
    let mut engine = new_graph::engine::GraphEngine::in_memory().unwrap();
    engine.replace_graph(dataset::modern_graph()).unwrap();
    let result = engine.gremlin("g.V().has('name','marko').sideEffect(__.out('knows').property('seen',true)).values('name')").await.unwrap();
    assert_eq!(format::lines_from_batch(&result.returned), vec!["marko"]);
    let result = engine
        .gremlin("g.V().has('seen',true).values('name')")
        .await
        .unwrap();
    let mut names = format::lines_from_batch(&result.returned);
    names.sort();
    assert_eq!(names, vec!["josh", "vadas"]);
    let result = engine
        .gremlin("g.V().has('name','lop').sideEffect(__.out().drop()).values('name')")
        .await
        .unwrap();
    assert_eq!(format::lines_from_batch(&result.returned), vec!["lop"]);
    let result = engine
        .gremlin("g.inject(1,2).sideEffect(__.addV('audit').discard()).count()")
        .await
        .unwrap();
    assert_eq!(format::lines_from_batch(&result.returned), vec!["2"]);
    let result = engine
        .gremlin("g.V().hasLabel('audit').count()")
        .await
        .unwrap();
    assert_eq!(format::lines_from_batch(&result.returned), vec!["2"]);
}

#[cfg(feature = "duckdb")]
#[tokio::test]
async fn unmatched_choice_emits_no_placeholder_row() {
    let mut engine = new_graph::engine::GraphEngine::in_memory().unwrap();
    let result = engine
        .gremlin("g.inject(0,1,2).choose(__.identity()).option(1L,__.constant('one'))")
        .await
        .unwrap();
    assert_eq!(format::lines_from_batch(&result.returned), vec!["one"]);
}

#[test]
fn side_effect_scalar_sack_is_scoped_to_child_split() {
    let graph = PropertyGraph::new();
    assert_rows(
        &graph,
        "g.withSack(1).inject(2).sideEffect(__.sack(Operator.sum)).sack()",
        &["1"],
    );
    assert_rows(
        &graph,
        "g.withSack(1).inject(2).sideEffect(__.sack(Operator.sum).discard()).sack()",
        &["1"],
    );
}

#[test]
fn correlated_children_run_at_unit_bulk_and_restore_parent_multiplicity() {
    let graph = PropertyGraph::new();
    assert_rows(
        &graph,
        "g.inject(1,1).barrier().map(__.count())",
        &["1", "1"],
    );
    assert_rows(
        &graph,
        "g.inject(1,1).barrier().flatMap(__.union(__.identity(),__.identity()).barrier())",
        &["1", "1", "1", "1"],
    );
    assert_rows(
        &graph,
        "g.inject(1,1).barrier().choose(__.count()).option(1L,__.identity())",
        &["1", "1"],
    );
    assert_rows(
        &graph,
        "g.inject(1,1).barrier().sideEffect(__.store('x')).cap('x').unfold().count()",
        &["1"],
    );
}

#[test]
fn implicit_where_projects_labels_and_shared_side_effect_values() {
    let graph = PropertyGraph::new();
    assert_rows(
        &graph,
        "g.inject(1).as('a').constant(2).where(P.gt('a'))",
        &["2"],
    );
    assert_rows(
        &graph,
        "g.inject(1,2).aggregate('a').where(P.within('a'))",
        &["1", "2"],
    );
    assert_rows(
        &graph,
        "g.withSideEffect('a',[1,2]).inject(1,3).where(P.within('a'))",
        &["1"],
    );
    assert_rows(
        &graph,
        "g.inject(1,1,2).groupCount('a').by(__.identity()).where(P.eq('a')).by(__.constant(3)).by(__.select(Column.values).sum(Scope.local)).count()",
        &["3"],
    );
}
