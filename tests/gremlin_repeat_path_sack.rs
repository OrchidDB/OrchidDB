use arrow::array::{ArrayRef, Int64Array};
use new_graph::ir::value::{Value, gremlin_set};
use new_graph::ir::{PropertyGraph, edges_from_columns, execute, nodes_from_columns};
use new_graph::language::gremlin::{GremlinPlanner, parse_traversal};
use std::sync::Arc;

#[path = "gremlin_case_runner/dataset.rs"]
mod dataset;

fn results(query: &str, graph: &PropertyGraph) -> Vec<String> {
    let traversal = parse_traversal(query).unwrap();
    let plan = GremlinPlanner::new().plan(&traversal).unwrap();
    let result = execute(&plan, graph).unwrap();
    (0..result.batch.num_rows())
        .map(|i| arrow::util::display::array_value_to_string(result.batch.column(0), i).unwrap())
        .collect()
}

#[test]
fn coalesce_constant_retains_scalar_history() {
    let graph = dataset::modern_graph();
    let prefix = "g.inject(0).V().both().coalesce(__.has('name','marko').both(),__.constant(0))";
    assert_eq!(
        results(&format!("{prefix}.cyclicPath().count()"), &graph),
        ["12"]
    );
    assert_eq!(
        results(&format!("{prefix}.simplePath().count()"), &graph),
        ["6"]
    );
    let traversal = parse_traversal(&format!("{prefix}.simplePath().path()")).unwrap();
    let plan = GremlinPlanner::new().plan(&traversal).unwrap();
    let mut paths = new_graph::ir::interpreter::execute_rows(&plan, &graph)
        .unwrap()
        .iter()
        .map(|row| {
            let Value::Path(items) = &row.bindings["current"] else {
                panic!("expected path")
            };
            items
                .iter()
                .map(|item| match item {
                    Value::Node { label, id } => match graph.node_property(label, *id, "name") {
                        Value::String(name) => name,
                        _ => panic!("missing name"),
                    },
                    Value::Int(0) => "0".into(),
                    other => panic!("unexpected path item {other:?}"),
                })
                .collect::<Vec<_>>()
                .join(",")
        })
        .collect::<Vec<_>>();
    paths.sort();
    assert_eq!(
        paths,
        [
            "0,josh,marko,lop",
            "0,josh,marko,vadas",
            "0,lop,marko,josh",
            "0,lop,marko,vadas",
            "0,vadas,marko,josh",
            "0,vadas,marko,lop",
        ]
    );
    assert_eq!(
        results("g.inject(1).constant(1).cyclicPath().count()", &graph),
        ["1"]
    );
    assert_eq!(
        results("g.inject(1).constant(1).simplePath().count()", &graph),
        ["0"]
    );
}

#[test]
fn path_labels_attach_to_positions_including_multiple_labels() {
    let graph = dataset::modern_graph();
    let traversal =
        parse_traversal("g.V().as('a','b').out().as('c').path().select(Column.keys)").unwrap();
    let plan = GremlinPlanner::new().plan(&traversal).unwrap();
    let rows = new_graph::ir::interpreter::execute_rows(&plan, &graph).unwrap();
    let expected = Value::List(vec![
        gremlin_set(vec![Value::String("a".into()), Value::String("b".into())]),
        gremlin_set(vec![Value::String("c".into())]),
    ]);
    assert_eq!(rows.len(), 6);
    assert!(
        rows.iter().all(|row| row.bindings["current"] == expected),
        "{rows:?}"
    );
    assert_eq!(
        results(
            "g.V().as('a','b').out().as('c').path().select(Column.keys).unfold().count()",
            &graph
        ),
        ["12"]
    );
}

#[test]
fn sacks_merge_with_and_without_bulk_and_normalize_inside_local() {
    let graph = dataset::modern_graph();
    assert_eq!(
        results("g.withBulk(false).V().out().barrier().count()", &graph),
        ["6"]
    );
    let mut sacks = results(
        "g.withBulk(false).withSack(1,Operator.sum).V().out().barrier().sack()",
        &graph,
    );
    sacks.sort();
    assert_eq!(sacks, ["1", "1", "1", "3"]);
    let sacks = results(
        "g.withBulk(false).withSack(1.0d,Operator.sum).V().has('name','marko').local(__.outE('knows').barrier(SackFunctions.Barrier.normSack).inV()).in('knows').barrier().sack()",
        &graph,
    );
    assert_eq!(sacks.len(), 1);
    assert_eq!(sacks[0].parse::<f64>().unwrap(), 1.0);
    let sacks = results(
        "g.withSack(1.0d,Operator.sum).V().has('name','marko').local(__.out('knows').barrier(Barrier.normSack)).in('knows').barrier().sack()",
        &graph,
    );
    assert_eq!(sacks.len(), 2);
    assert!(sacks.iter().all(|s| s.parse::<f64>().unwrap() == 1.0));
}

#[test]
fn normalized_sack_barrier_weights_already_merged_bulk() {
    let sacks = results(
        "g.withSack(1.0d,Operator.sum).inject(1,1,2).barrier(Barrier.normSack).sack()",
        &PropertyGraph::new(),
    );
    let mut values = sacks
        .iter()
        .map(|s| s.parse::<f64>().unwrap())
        .collect::<Vec<_>>();
    values.sort_by(f64::total_cmp);
    assert_eq!(values, [0.2, 0.8, 0.8]);
}

#[test]
fn compact_repeat_counts_walk_multiplicity_without_retaining_paths() {
    let mut graph = PropertyGraph::new();
    graph.add_nodes(nodes_from_columns(
        "P",
        vec![("n", Arc::new(Int64Array::from(vec![0])) as ArrayRef)],
    ));
    graph
        .add_edges(edges_from_columns(
            "R",
            "P",
            "P",
            vec![0, 0],
            vec![0, 0],
            vec![],
        ))
        .unwrap();
    assert_eq!(
        results("g.V().repeat(__.out()).times(50).count()", &graph),
        ["1125899906842624"]
    );
    assert_eq!(
        results("g.V().repeat(__.out()).times(50).limit(7).count()", &graph),
        ["7"]
    );
    assert_eq!(
        results(
            "g.V().repeat(__.out()).times(50).range(5,12).count()",
            &graph
        ),
        ["7"]
    );
    assert_eq!(
        results("g.V().repeat(__.out()).times(50).tail(7).count()", &graph),
        ["7"]
    );
    assert_eq!(
        results(
            "g.V().repeat(__.out()).times(5).as('a').out('R').as('b').select('a','b').count()",
            &graph
        ),
        ["64"]
    );
    assert_eq!(
        results("g.V().repeat(__.out()).times(4).path().count()", &graph),
        ["16"]
    );
    assert_eq!(
        results(
            "g.withSack(1,Operator.sum).V().out().barrier().limit(1).count()",
            &graph
        ),
        ["1"]
    );
    assert_eq!(
        results(
            "g.withSack(1,Operator.sum).V().out().barrier().range(1,2).count()",
            &graph
        ),
        ["1"]
    );
    assert_eq!(
        results(
            "g.withSack(1,Operator.sum).V().out().barrier().tail(1).count()",
            &graph
        ),
        ["1"]
    );
}

#[test]
fn terminal_repeat_rows_survive_rejecting_emit_predicate() {
    let graph = dataset::modern_graph();
    assert_eq!(
        results(
            "g.inject(1).repeat(__.identity()).times(2).emit(__.is(2)).count()",
            &graph
        ),
        ["1"]
    );
    let mut values = results(
        "g.V().has('name','lop').repeat(__.both('created')).until(__.loops().is(40)).emit(__.repeat(__.in('knows')).emit(__.loops().is(1))).dedup().values('name')",
        &graph,
    );
    values.sort();
    assert_eq!(values, ["josh", "lop", "ripple"]);
}

#[test]
fn inner_repeat_restores_enclosing_anonymous_loop_counter() {
    let graph = PropertyGraph::new();
    assert_eq!(
        results(
            "g.inject(1).repeat(__.repeat(__.identity()).times(3).loops()).times(2)",
            &graph
        ),
        ["1"]
    );
}

#[test]
fn prefix_until_does_not_replay_its_input_writer() {
    let graph = PropertyGraph::new();
    assert_eq!(
        results(
            "g.inject(1).addV('x').until(__.constant(true)).repeat(__.identity()).count()",
            &graph
        ),
        ["1"]
    );
    assert_eq!(results("g.V().hasLabel('x').count()", &graph), ["1"]);
    assert_eq!(
        results(
            "g.inject(1,2).aggregate('a').until(__.constant(true)).repeat(__.identity()).cap('a').unfold().count()",
            &graph
        ),
        ["2"]
    );
    assert_eq!(
        results(
            "g.inject(1).until(__.is(1)).emit().repeat(__.identity()).count()",
            &graph
        ),
        ["1"]
    );
}

fn native_values(query: &str) -> Vec<Value> {
    let traversal = parse_traversal(query).unwrap();
    let plan = GremlinPlanner::new().plan(&traversal).unwrap();
    new_graph::ir::interpreter::execute_rows(&plan, &PropertyGraph::new())
        .unwrap().into_iter().map(|row| row.bindings["current"].clone()).collect()
}

#[test]
fn repeat_retracts_expired_child_only_labels_using_the_default_strategy() {
    // Pinned TinkerPop: the project child reads v, but the body's later
    // select(t) only retains direct body scope keys p and t. Reattaching v
    // next iteration starts a new history. This scalar example is independent
    // of the integrated shortest-path fixture and its vertex names.
    let body = "__.filter(__.loops().is(P.lt(2))).constant(1).as('v').project('p').by(__.select(Pop.all,'v')).as('t').select('t').select('p').aggregate('x')";
    let query = format!("g.inject(0).as('v').repeat({body}).cap('x').unfold()");
    assert_eq!(native_values(&query), [
        Value::List(vec![Value::Int(0), Value::Int(1)]),
        Value::List(vec![Value::Int(1)]),
    ]);
    let disabled = query.replacen("g.inject", "g.withoutStrategies(PathRetractionStrategy).inject", 1);
    assert_eq!(native_values(&disabled), [
        Value::List(vec![Value::Int(0), Value::Int(1)]),
        Value::List(vec![Value::Int(0), Value::Int(1), Value::Int(1)]),
    ]);
    let path_observing = query.replace(".aggregate('x')", ".sideEffect(__.path()).aggregate('x')");
    assert_eq!(native_values(&path_observing), native_values(&disabled));
}

#[test]
fn label_retraction_preserves_repeat_siblings_and_future_pop_reads() {
    let body = "__.constant(1).as('v').project('p').by(__.select(Pop.all,'v')).as('t').select('t').select('p')";
    let expected = [Value::List(vec![Value::Int(0), Value::Int(1), Value::Int(1)])];
    // Fixed repeats are logically unrolled before label retraction in
    // TinkerPop. Until/emit children instead share their label keepers.
    assert_eq!(native_values(&format!("g.inject(0).as('v').repeat({body}).times(2)")), expected);
    assert_eq!(native_values(&format!("g.inject(0).as('v').repeat({body}).until(__.loops().is(2))")), expected);
    let query = "g.inject(0).as('v').constant(1).as('v').select(Pop.first,'v').select(Pop.last,'v').select(Pop.all,'v')";
    assert_eq!(native_values(query), [Value::List(vec![Value::Int(0), Value::Int(1)])]);
    assert_eq!(native_values("g.inject(1).as('a').constant(2).as('b').select('b').where('a',P.lt('b')).select('a')"), [Value::Int(1)]);
    assert_eq!(native_values("g.inject(1).as('a').constant(2).as('b').select('b').where(__.as('a').is(1)).select('a')"), [Value::Int(1)]);
    assert_eq!(native_values("g.inject(null).as('a').constant(1).as('b').select('b').select('a')"), [Value::Null]);
    assert_eq!(native_values("g.inject(1).as('a').select('a').match(__.as('a').constant(2).as('b')).select('a')"), [Value::Int(1)]);
}
