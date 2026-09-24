//! Relational executor regression tests. The reference dispatcher provides an
//! independent oracle; production execution must report a DataFusion plan.
use new_graph::ir::jvm::JvmExecution;
use new_graph::ir::rel::runtime::execute_rows_with_jvm;
use new_graph::ir::{GraphPlan, PropertyGraph, Value};
use new_graph::language::gremlin::{GremlinPlanner, parse_traversal};
use std::collections::BTreeMap;

fn plan(query: &str) -> GraphPlan {
    GremlinPlanner::new()
        .plan(&parse_traversal(query).unwrap())
        .unwrap()
}
fn graph() -> PropertyGraph {
    let graph = PropertyGraph::new();
    let a = graph.insert_node(
        "person",
        BTreeMap::from([
            ("name".into(), Value::String("a".into())),
            ("age".into(), Value::Int(29)),
        ]),
    );
    let b = graph.insert_node(
        "person",
        BTreeMap::from([
            ("name".into(), Value::String("b".into())),
            ("age".into(), Value::Int(35)),
        ]),
    );
    let c = graph.insert_node(
        "software",
        BTreeMap::from([("name".into(), Value::String("c".into()))]),
    );
    graph.insert_edge("knows", &a, &b, BTreeMap::new()).unwrap();
    graph
        .insert_edge("created", &a, &c, BTreeMap::new())
        .unwrap();
    graph
}
#[tokio::test]
async fn typed_traversers_and_correlated_state_match_reference() {
    for query in [
        "g.V().groupCount().by('age')",
        "g.V().group().by(__.label()).by(__.count())",
        "g.V().choose(__.has('name','a'),__.values('name'),__.identity())",
        "g.V().outE().inV().values('name')",
        "g.V().local(__.out().count())",
        "g.V().local(__.out().has('name','b').values('age'))",
        "g.V().filter(__.out().has('name','b')).values('name')",
        "g.V().has('name','a').as('a').out().as('a').select(Pop.first,'a')",
        "g.inject(null).coalesce(__.identity(),__.constant('fallback'))",
        "g.inject(1,2,3).store('x').limit(1).cap('x')",
        "g.inject(1).repeat(__.groupCount('m')).times(3).cap('m')",
    ] {
        let plan = plan(query);
        let graph = graph();
        let mut expected = new_graph::ir::interpreter::execute_rows(&plan, &graph.clone()).unwrap();
        let (mut actual, stats) = execute_rows_with_jvm(&plan, &graph, JvmExecution::default())
            .await
            .unwrap_or_else(|e| panic!("{query}: {e}"));
        assert!(!stats.physical_plan.is_empty(), "{query}");
        assert!(
            !stats.physical_plan.contains("GraphJvm"),
            "{}",
            stats.physical_plan
        );
        // These traversals do not request a result order. Compare the row
        // multiset while retaining exact types, hidden bindings, and bulk.
        actual.sort_by_key(|r| format!("{:?}:{}", r.bindings, r.bulk));
        expected.sort_by_key(|r| format!("{:?}:{}", r.bindings, r.bulk));
        assert_eq!(actual.len(), expected.len(), "{query}");
        for (actual, expected) in actual.iter().zip(expected) {
            assert_eq!(actual.bulk, expected.bulk, "{query}");
            assert_eq!(actual.bindings, expected.bindings, "{query}");
        }
    }
}
#[tokio::test]
#[cfg(feature = "duckdb")]
async fn branch_scans_observe_prior_writes_and_outer_rollback() {
    let mut engine = new_graph::engine::GraphEngine::in_memory().unwrap();
    let result = engine
        .gremlin("g.inject(1).union(__.addV('person'),__.V()).count()")
        .await
        .unwrap();
    assert_eq!(result.stats.interpreted_ops, 0);
    assert!(result.stats.datafusion_ops > 0);
    let rows: serde_json::Value = serde_json::from_str(
        &result.returned.batch.schema().metadata()["crabgraph.gremlin.typed_rows.v1"],
    )
    .unwrap();
    assert_eq!(rows[0][0]["value"], 2);
    engine.begin().unwrap();
    engine.gremlin("g.addV('pending')").await.unwrap();
    engine.rollback().unwrap();
    let result = engine
        .gremlin("g.V().hasLabel('pending').count()")
        .await
        .unwrap();
    let rows: serde_json::Value = serde_json::from_str(
        &result.returned.batch.schema().metadata()["crabgraph.gremlin.typed_rows.v1"],
    )
    .unwrap();
    assert_eq!(rows[0][0]["value"], 0);
}
#[test]
fn graph_checkpoints_isolate_writes_in_both_directions() {
    let graph = graph();
    let checkpoint = graph.clone();
    graph.insert_node("later", BTreeMap::new());
    assert!(checkpoint.node_ids("later").is_err());
    checkpoint.insert_node("checkpoint", BTreeMap::new());
    assert!(graph.node_ids("checkpoint").is_err());
}

#[tokio::test]
#[cfg(feature = "duckdb")]
async fn reused_dag_resources_do_not_reuse_graph_contents() {
    let mut first = new_graph::engine::GraphEngine::in_memory().unwrap();
    let mut second = new_graph::engine::GraphEngine::in_memory().unwrap();
    async fn count(engine: &mut new_graph::engine::GraphEngine) -> String {
        let result = engine.gremlin("g.V().count()").await.unwrap();
        result.returned.batch.schema().metadata()["crabgraph.gremlin.typed_rows.v1"].clone()
    }
    first.replace_graph(graph()).unwrap();
    let original = count(&mut first).await;
    let empty = count(&mut second).await;
    assert_ne!(original, empty);
    for _ in 0..3 {
        first.replace_graph(PropertyGraph::new()).unwrap();
        assert_eq!(count(&mut first).await, empty);
        first.replace_graph(graph()).unwrap();
        assert_eq!(count(&mut first).await, original);
        assert_eq!(count(&mut second).await, empty);
    }
    first.begin().unwrap();
    first.gremlin("g.addV('pending')").await.unwrap();
    assert_ne!(count(&mut first).await, original);
    first.rollback().unwrap();
    assert_eq!(count(&mut first).await, original);
    assert!(
        first
            .gremlin("g.inject(1).fail('expected error')")
            .await
            .is_err()
    );
    assert_eq!(count(&mut first).await, original);
}

#[tokio::test]
async fn fused_unary_kernels_preserve_exact_rows_and_bulk() {
    let query = plan("g.inject(1,2,3).map(__.constant(5)).identity().constant(7)");
    let graph = PropertyGraph::new();
    let expected = new_graph::ir::interpreter::execute_rows(&query, &graph).unwrap();
    let (actual, stats) = execute_rows_with_jvm(&query, &graph, JvmExecution::default())
        .await
        .unwrap();
    assert!(
        stats.physical_plan.contains("Fused("),
        "{}",
        stats.physical_plan
    );
    assert_eq!(actual.len(), expected.len());
    for (a, b) in actual.iter().zip(expected) {
        assert_eq!(a.bindings, b.bindings);
        assert_eq!(a.bulk, b.bulk);
    }
}

#[tokio::test]
async fn ranking_windows_allow_unordered_and_ordered_dedup() {
    for query in [
        "g.V().dedup().values('name')",
        "g.V().order().by('name').dedup().values('name')",
    ] {
        let graph = graph();
        let plan = plan(query);
        let expected = new_graph::ir::interpreter::execute_rows(&plan, &graph.clone()).unwrap();
        let (actual, _) = execute_rows_with_jvm(&plan, &graph, JvmExecution::default()).await.unwrap();
        // Compare observable values and multiplicity; internal dedup keys use
        // different physical representations in SQL and the reference engine.
        let visible = |rows: Vec<new_graph::ir::interpreter::Row>| {
            let mut values: Vec<_> = rows.into_iter().map(|row| (format!("{:?}", row.bindings["current"]), row.bulk)).collect();
            values.sort();
            values
        };
        assert_eq!(visible(actual), visible(expected), "{query}");
    }
}

#[tokio::test]
async fn batched_apply_preserves_occurrences_bulk_and_optional_results() {
    use new_graph::ir::expr::{IrExpr, Lit, BinaryOp};
    use new_graph::ir::plan::{Node, ApplyKind, ProjectMode, ProjectErrorPolicy, ProjectionItem};
    use new_graph::ir::policy::{GraphPlanPolicy, OptionalMissing};
    for kind in [ApplyKind::Inner,ApplyKind::Optional,ApplyKind::Semi,ApplyKind::Anti,ApplyKind::Scalar] {
        let right=Node::GraphProject {
            mode:ProjectMode::PreserveVisible,error_policy:ProjectErrorPolicy::PropagateError,
            items:vec![ProjectionItem {alias:"answer".into(),expr:IrExpr::Binding("current".into())}],
            input:Box::new(Node::GraphFilter {
                condition:IrExpr::Binary {op:BinaryOp::Eq,lhs:Box::new(IrExpr::Binding("current".into())),rhs:Box::new(IrExpr::Lit(Lit::Int(1)))},
                input:Box::new(Node::GraphCorrelate {bindings:vec!["current".into()]}),
            }),
        };
        let root=Node::GraphApply {kind,correlation:vec!["current".into()],outputs:vec!["answer".into()],optional_missing:OptionalMissing::Null,
            left:Box::new(Node::GraphValues {bindings:vec!["current".into()],rows:vec![vec![Value::Int(1)],vec![Value::Int(1)],vec![Value::Int(2)]],bulk:Some(vec![2,3,4])}),right:Box::new(right)};
        let plan=GraphPlan::new(GraphPlanPolicy::gremlin(),root);
        let graph=PropertyGraph::new();
        let expected=new_graph::ir::interpreter::execute_rows(&plan,&graph).unwrap();
        let (actual,stats)=execute_rows_with_jvm(&plan,&graph,JvmExecution::default()).await.unwrap();
        assert!(stats.physical_plan.contains("BatchableLateralApply"),"{}",stats.physical_plan);
        assert_eq!(actual.len(),expected.len(),"{kind:?}");
        for (a,b) in actual.iter().zip(expected) {assert_eq!(a.bindings,b.bindings,"{kind:?}");assert_eq!(a.bulk,b.bulk,"{kind:?}");}
    }
}

#[tokio::test]
async fn projection_composition_preserves_shadowing_and_missing_bindings() {
    use new_graph::ir::expr::{IrExpr, Lit};
    use new_graph::ir::plan::{Node, ProjectMode, ProjectErrorPolicy, ProjectionItem};
    use new_graph::ir::policy::GraphPlanPolicy;
    let item=|alias:&str,expr|ProjectionItem {alias:alias.into(),expr};
    for mode in [ProjectMode::PreserveVisible,ProjectMode::ReplaceScope] {
        let root=Node::GraphProject {mode,error_policy:ProjectErrorPolicy::PropagateError,
            items:vec![item("x",IrExpr::Binding("y".into())),item("missing",IrExpr::Binding("missing".into())),item("copy",IrExpr::Binding("y".into()))],
            input:Box::new(Node::GraphProject {mode:ProjectMode::PreserveVisible,error_policy:ProjectErrorPolicy::PropagateError,
                items:vec![item("x",IrExpr::Lit(Lit::Int(9))),item("y",IrExpr::Binding("x".into()))],
                input:Box::new(Node::GraphValues {bindings:vec!["x".into()],rows:vec![vec![Value::Int(1)]],bulk:Some(vec![3])})})};
        let plan=GraphPlan::new(GraphPlanPolicy::gremlin(),root); let graph=PropertyGraph::new();
        let expected=new_graph::ir::interpreter::execute_rows(&plan,&graph).unwrap();
        let (actual,stats)=execute_rows_with_jvm(&plan,&graph,JvmExecution::default()).await.unwrap();
        assert!(stats.physical_plan.contains("CollapsedProject"),"{}",stats.physical_plan);
        assert_eq!(actual[0].bindings,expected[0].bindings);assert_eq!(actual[0].bulk,3);
        assert_eq!(actual[0].bindings["missing"],Value::Null);
    }
}

#[tokio::test]
async fn required_bindings_preserve_projection_inputs_and_errors() {
    use new_graph::ir::expr::{IrExpr, Lit, BinaryOp};
    use new_graph::ir::plan::{Node, ProjectMode, ProjectErrorPolicy, ProjectionItem};
    use new_graph::ir::policy::{GraphPlanPolicy,ResultForm,PropertyMissing};
    let source=||Node::GraphValues {bindings:vec!["x".into(),"unused".into()],rows:vec![vec![Value::Int(2),Value::String("large payload".repeat(100))]],bulk:Some(vec![2])};
    let project=|expr|Node::GraphProject {mode:ProjectMode::PreserveVisible,error_policy:ProjectErrorPolicy::PropagateError,
        items:vec![ProjectionItem {alias:"answer".into(),expr}],input:Box::new(source())};
    let graph=PropertyGraph::new();
    let plan=GraphPlan::new(GraphPlanPolicy::gremlin(),Node::GraphReturn {fields:vec!["answer".into()],result_form:ResultForm::TraverserStream,
        input:Box::new(project(IrExpr::Binary {op:BinaryOp::Add,lhs:Box::new(IrExpr::Binding("x".into())),rhs:Box::new(IrExpr::Lit(Lit::Int(1)))}))});
    let expected=new_graph::ir::interpreter::execute(&plan,&graph).unwrap();
    let (actual,stats)=new_graph::ir::rel::runtime::execute(&plan,&graph,None).await.unwrap();
    assert_eq!(actual.batch,expected.batch);
    assert!(stats.physical_plan.contains("Pruned("),"{}",stats.physical_plan);
    // A filter rejecting every row must not suppress a preceding expression
    // error, even when the failed alias is absent from the return fields.
    let error_plan=GraphPlan::new(GraphPlanPolicy::gremlin(),Node::GraphReturn {fields:vec!["x".into()],result_form:ResultForm::TraverserStream,
        input:Box::new(Node::GraphFilter {condition:IrExpr::Lit(Lit::Bool(false)),input:Box::new(project(IrExpr::Property {binding:"missing".into(),name:"p".into(),policy:PropertyMissing::Error}))})});
    assert!(new_graph::ir::interpreter::execute(&error_plan,&graph).is_err());
    let error=new_graph::ir::rel::runtime::execute(&error_plan,&graph,None).await.unwrap_err();
    assert!(error.contains("missing") || error.contains("property"),"{error}");
}

#[tokio::test]
async fn pure_filter_moves_before_unrelated_projection() {
    use new_graph::ir::expr::{IrExpr,Lit,BinaryOp};
    use new_graph::ir::plan::{Node,ProjectMode,ProjectErrorPolicy,ProjectionItem};
    use new_graph::ir::policy::GraphPlanPolicy;
    let root=Node::GraphFilter {condition:IrExpr::Binary {op:BinaryOp::Eq,lhs:Box::new(IrExpr::Binding("x".into())),rhs:Box::new(IrExpr::Lit(Lit::Int(1)))},
        input:Box::new(Node::GraphProject {mode:ProjectMode::PreserveVisible,error_policy:ProjectErrorPolicy::PropagateError,
            items:vec![ProjectionItem {alias:"answer".into(),expr:IrExpr::Lit(Lit::String("payload".repeat(100)))}],
            input:Box::new(Node::GraphValues {bindings:vec!["x".into()],rows:vec![vec![Value::Int(1)],vec![Value::Int(2)]],bulk:Some(vec![2,3])})})};
    let plan=GraphPlan::new(GraphPlanPolicy::gremlin(),root);let graph=PropertyGraph::new();
    let expected=new_graph::ir::interpreter::execute_rows(&plan,&graph).unwrap();
    let (actual,stats)=execute_rows_with_jvm(&plan,&graph,JvmExecution::default()).await.unwrap();
    assert!(stats.physical_plan.contains("Values -> Filter"),"{}",stats.physical_plan);
    assert_eq!(actual.len(),1);assert_eq!(actual[0].bindings,expected[0].bindings);assert_eq!(actual[0].bulk,2);
}

#[tokio::test]
async fn batched_fanout_keeps_duplicate_outputs_and_parent_bulk() {
    use new_graph::ir::expr::IrExpr;
    use new_graph::ir::plan::{Node,ApplyKind};
    use new_graph::ir::policy::{GraphPlanPolicy,OptionalMissing};
    let list=Value::List(vec![Value::Int(1),Value::Int(1)]);
    let plan=GraphPlan::new(GraphPlanPolicy::gremlin(),Node::GraphApply {kind:ApplyKind::Inner,correlation:vec!["current".into()],outputs:vec!["answer".into()],optional_missing:OptionalMissing::Null,
        left:Box::new(Node::GraphValues {bindings:vec!["current".into()],rows:vec![vec![list.clone()],vec![list]],bulk:Some(vec![2,3])}),
        right:Box::new(Node::GraphUnwind {input_expr:IrExpr::Binding("current".into()),bind:"answer".into(),outer:false,
            input:Box::new(Node::GraphCorrelate {bindings:vec!["current".into()]})})});
    let graph=PropertyGraph::new();let expected=new_graph::ir::interpreter::execute_rows(&plan,&graph).unwrap();
    let (actual,stats)=execute_rows_with_jvm(&plan,&graph,JvmExecution::default()).await.unwrap();
    assert!(stats.physical_plan.contains("BatchableLateralApply"));assert_eq!(actual.len(),4);
    for (a,b) in actual.iter().zip(expected) {assert_eq!(a.bindings,b.bindings);assert_eq!(a.bulk,b.bulk);}
}
