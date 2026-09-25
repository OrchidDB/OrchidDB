#![cfg(feature = "duckdb")]
use orchiddb::ir::jvm::{JvmConfig, JvmExecution, JvmMode, JvmOperation};
use orchiddb::ir::{
    GraphPlan, GraphPlanPolicy, IrExpr, Node, ProjectionItem, PropertyGraph, Value,
};
use std::collections::BTreeMap;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};
fn execute_rows_with_jvm(
    plan: &GraphPlan,
    graph: &PropertyGraph,
    jvm: JvmExecution,
) -> Result<Vec<orchiddb::ir::interpreter::Row>, String> {
    tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(orchiddb::ir::rel::runtime::execute_rows_with_jvm(
            plan, graph, jvm,
        ))
        .map(|(rows, _)| rows)
}

fn input(values: Vec<Value>) -> Node {
    Node::GraphValues {
        bindings: vec!["current".into()],
        rows: values.into_iter().map(|value| vec![value]).collect(),
        bulk: None,
    }
}
fn jvm(input: Node, script: &str, mode: JvmMode) -> Node {
    Node::GraphJvm {
        operation: JvmOperation {
            script: script.into(),
            mode,
            output: "current".into(),
            arguments: vec![ProjectionItem {
                alias: "current".into(),
                expr: IrExpr::Binding("current".into()),
            }],
        },
        input: input.boxed(),
    }
}
fn plan(node: Node) -> GraphPlan {
    GraphPlan::new(GraphPlanPolicy::gremlin(), node)
}
fn execution() -> JvmExecution {
    JvmExecution {
        config: Some(
            JvmConfig::from_env()
                .expect("Set ORCHIDDB_JVM_CLASSPATH to the built production JVM and dependencies"),
        ),
        deadline: Some(Instant::now() + Duration::from_secs(40)),
        ..Default::default()
    }
}

#[test]
fn jvm_nodes_round_trip_through_optimizer_and_are_effect_fences() {
    let plan = plan(jvm(input(vec![Value::Int(1)]), "current + 1", JvmMode::Map));
    let logical = orchiddb::ir::df::to_logical_plan(&plan).unwrap();
    let rebuilt =
        orchiddb::ir::df::from_logical_plan_with_policy(plan.policy.clone(), &logical).unwrap();
    assert_eq!(plan, rebuilt);
    assert!(orchiddb::ir::exec::contains_mutation(&plan.root));
    assert!(orchiddb::ir::analysis::validate_read_only(&plan).is_err());
    assert!(orchiddb::ir::explain(&plan).contains("GraphJvm"));
}

#[test]
fn gremlin_call_lowers_to_jvm_ir() {
    let parsed = orchiddb::language::gremlin::parser::parse_traversal(
        "g.inject(2).call('orchiddb.jvm',['script':'current + 3'])",
    )
    .unwrap();
    let plan = orchiddb::language::gremlin::planner::GremlinPlanner::new()
        .plan(&parsed)
        .unwrap();
    assert!(orchiddb::ir::jvm::contains_jvm(&plan.root));
}

#[test]
#[ignore = "requires the production JVM classpath"]
fn scalar_filter_flatmap_preserve_bulk_and_hidden_bindings() {
    let source = Node::GraphValues {
        bindings: vec!["current".into(), "__sack".into(), "__path".into()],
        rows: vec![vec![
            Value::Long(9),
            Value::String("sack".into()),
            Value::Path(vec![Value::Long(9)]),
        ]],
        bulk: Some(vec![7]),
    };
    let mapped = jvm(source, "current + 2L", JvmMode::Map);
    let filtered = jvm(mapped, "current == 11L", JvmMode::Filter);
    let expanded = jvm(
        filtered,
        "[current, null, new BigInteger('123456789012345678901234567890')]",
        JvmMode::FlatMap,
    );
    let rows = execute_rows_with_jvm(&plan(expanded), &PropertyGraph::new(), execution()).unwrap();
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0].get("current"), Value::Long(11));
    assert_eq!(rows[1].get("current"), Value::Null);
    assert_eq!(
        rows[2].get("current"),
        Value::BigInt("123456789012345678901234567890".parse().unwrap())
    );
    for row in rows {
        assert_eq!(row.bulk, 7);
        assert_eq!(row.get("__sack"), Value::String("sack".into()));
        assert_eq!(row.get("__path"), Value::Path(vec![Value::Long(9)]));
    }
}

#[test]
#[ignore = "requires the production JVM classpath"]
fn graph_identity_uncommitted_writes_and_failure_rollback() {
    let graph = PropertyGraph::new();
    let vertex = graph.insert_node(
        "person",
        BTreeMap::from([("name".into(), Value::String("before".into()))]),
    );
    let change = jvm(
        input(vec![vertex.clone()]),
        "current.property('name', 'after'); current",
        JvmMode::Map,
    );
    let rows = execute_rows_with_jvm(&plan(change.clone()), &graph, execution()).unwrap();
    assert_eq!(rows[0].get("current"), vertex);
    assert_eq!(
        graph.node_property("person", 0, "name"),
        Value::String("after".into())
    );
    graph
        .set_property(&vertex, "name", Value::String("caller".into()))
        .unwrap();
    let failing = jvm(
        change,
        "throw new IllegalStateException('failure after previous JVM writes')",
        JvmMode::Map,
    );
    assert!(execute_rows_with_jvm(&plan(failing), &graph, execution()).is_err());
    assert_eq!(
        graph.node_property("person", 0, "name"),
        Value::String("caller".into())
    );
    let failing = jvm(
        input(vec![vertex]),
        "current.property('name','lost'); throw new IllegalStateException('failure')",
        JvmMode::Map,
    );
    assert!(execute_rows_with_jvm(&plan(failing), &graph, execution()).is_err());
    assert_eq!(
        graph.node_property("person", 0, "name"),
        Value::String("caller".into())
    );
}

#[test]
#[ignore = "requires the production JVM classpath"]
fn cancellation_and_deadline_terminate_worker_without_publishing_writes() {
    let graph = PropertyGraph::new();
    let vertex = graph.insert_node("person", BTreeMap::new());
    let work = plan(jvm(
        input(vec![vertex]),
        "current.property('cancelled',true); while(true) { Thread.sleep(10) }; current",
        JvmMode::Map,
    ));
    let cancelled = Arc::new(AtomicBool::new(false));
    let signal = cancelled.clone();
    let thread = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(2));
        signal.store(true, Ordering::Release);
    });
    let mut runtime = execution();
    runtime.cancelled = cancelled;
    let started = Instant::now();
    let error = execute_rows_with_jvm(&work, &graph, runtime).unwrap_err();
    thread.join().unwrap();
    assert!(error.to_string().contains("cancelled"), "{error}");
    assert!(started.elapsed() < Duration::from_secs(10));
    assert_eq!(graph.node_property("person", 0, "cancelled"), Value::Null);
    let mut runtime = execution();
    runtime.deadline = Some(Instant::now() + Duration::from_millis(100));
    assert!(
        execute_rows_with_jvm(&work, &graph, runtime)
            .unwrap_err()
            .to_string()
            .contains("deadline")
    );
}

#[test]
#[ignore = "requires the production JVM classpath"]
fn transactions_cannot_commit_inside_a_node_and_null_policy_is_preserved() {
    let graph = PropertyGraph::new();
    let vertex = graph.insert_node(
        "person",
        BTreeMap::from([("name".into(), Value::String("caller".into()))]),
    );
    let work = plan(jvm(
        input(vec![vertex.clone()]),
        "current.property('name','lost'); graph.tx().commit(); current",
        JvmMode::Map,
    ));
    assert!(execute_rows_with_jvm(&work, &graph, execution()).is_err());
    assert_eq!(
        graph.node_property("person", 0, "name"),
        Value::String("caller".into())
    );
    let work = plan(jvm(
        input(vec![vertex]),
        "current.property('name',null); graph.features().vertex().supportsNullPropertyValues()",
        JvmMode::Map,
    ));
    let rows = execute_rows_with_jvm(&work, &graph, execution()).unwrap();
    assert_eq!(rows[0].get("current"), Value::Bool(false));
    assert!(!graph.supports_null_property_values());
}

#[test]
#[ignore = "requires the production JVM classpath"]
fn repeat_body_uses_the_correlated_frontier() {
    let parsed = orchiddb::language::gremlin::parser::parse_traversal(
        "g.inject(2).repeat(__.call('orchiddb.jvm',['script':'current + 1'])).times(2)",
    )
    .unwrap();
    let plan = orchiddb::language::gremlin::planner::GremlinPlanner::new()
        .plan(&parsed)
        .unwrap();
    let rows = execute_rows_with_jvm(&plan, &PropertyGraph::new(), execution()).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get("current"), Value::Int(4));
}

#[cfg(feature = "duckdb")]
#[tokio::test]
#[ignore = "requires the production JVM classpath"]
async fn sql_prefix_then_datafusion_jvm_and_datafusion_suffix() {
    use orchiddb::ir::plan::ProjectErrorPolicy;
    use orchiddb::ir::{BinaryOp, ProjectMode, ResultForm};
    fn add(input: Node, n: i64) -> Node {
        Node::GraphProject {
            mode: ProjectMode::PreserveVisible,
            error_policy: ProjectErrorPolicy::PropagateError,
            items: vec![ProjectionItem {
                alias: "current".into(),
                expr: IrExpr::Binary {
                    op: BinaryOp::Add,
                    lhs: Box::new(IrExpr::Binding("current".into())),
                    rhs: Box::new(IrExpr::lit_int(n)),
                },
            }],
            input: input.boxed(),
        }
    }
    let graph = PropertyGraph::new();
    let prefix = add(input(vec![Value::Long(2), Value::Long(4)]), 1);
    let compute = jvm(
        prefix,
        "graph.addVertex('marker'); current * 2L",
        JvmMode::Map,
    );
    let suffix = add(compute, 10);
    let query = plan(Node::GraphReturn {
        fields: vec!["current".into()],
        result_form: ResultForm::RowSet,
        input: suffix.boxed(),
    });
    let (result, stats) = orchiddb::ir::rel::runtime::execute(&query, &graph, None)
        .await
        .unwrap();
    assert_eq!(
        stats.duckdb_regions, 1,
        "only the prefix should execute in SQL: {stats:?}"
    );
    assert!(
        stats.physical_plan.contains("JvmExec"),
        "{}",
        stats.physical_plan
    );
    assert!(
        stats.physical_plan.contains("DuckDbExec"),
        "{}",
        stats.physical_plan
    );
    assert!(!stats.physical_plan.contains("GraphJvm"));
    let values: Vec<String> = (0..result.batch.num_rows())
        .map(|r| arrow::util::display::array_value_to_string(result.batch.column(0), r).unwrap())
        .collect();
    assert_eq!(values, vec!["16", "20"]);
    assert_eq!(
        graph.node_ids("marker").unwrap().len(),
        2,
        "JVM effects must not be replayed"
    );
}

#[test]
#[ignore = "requires the production JVM classpath"]
fn datafusion_failure_after_jvm_restores_caller_state() {
    use orchiddb::ir::ProjectMode;
    use orchiddb::ir::plan::ProjectErrorPolicy;
    let graph = PropertyGraph::new();
    let vertex = graph.insert_node(
        "person",
        BTreeMap::from([("name".into(), Value::String("caller".into()))]),
    );
    let compute = jvm(
        input(vec![vertex]),
        "current.property('name','lost'); current",
        JvmMode::Map,
    );
    let suffix = Node::GraphProject {
        mode: ProjectMode::PreserveVisible,
        error_policy: ProjectErrorPolicy::PropagateError,
        items: vec![ProjectionItem {
            alias: "current".into(),
            expr: IrExpr::Call {
                name: "deliberate_unknown_function".into(),
                args: vec![],
            },
        }],
        input: compute.boxed(),
    };
    assert!(execute_rows_with_jvm(&plan(suffix), &graph, execution()).is_err());
    assert_eq!(
        graph.node_property("person", 0, "name"),
        Value::String("caller".into())
    );
}

#[test]
#[ignore = "requires the production JVM classpath"]
fn graphcomputer_fragment_uses_private_native_result_graph() {
    let graph = PropertyGraph::new();
    let a = graph.insert_node("person", BTreeMap::new());
    let b = graph.insert_node("person", BTreeMap::new());
    graph.insert_edge("knows", &a, &b, BTreeMap::new()).unwrap();
    let compute = jvm(
        input(vec![Value::Null]),
        "g.withComputer().V().pageRank().values('gremlin.pageRankVertexProgram.pageRank')",
        JvmMode::FlatMap,
    );
    let rows = execute_rows_with_jvm(&plan(compute), &graph, execution()).unwrap();
    assert_eq!(rows.len(), 2);
    assert!(
        rows.iter().all(
            |r| matches!(r.get("current"),Value::Float(value) if value.is_finite() && value>0.0)
        )
    );
    assert_eq!(
        graph.node_property("person", 0, "gremlin.pageRankVertexProgram.pageRank"),
        Value::Null,
        "NEW result graph must not mutate the source"
    );
    let compute = jvm(
        input(vec![Value::Null]),
        "g.withComputer().V().pageRank()",
        JvmMode::FlatMap,
    );
    let error = execute_rows_with_jvm(&plan(compute), &graph, execution()).unwrap_err();
    assert!(
        error.to_string().contains("execution-local graph elements"),
        "{error}"
    );
    assert_eq!(
        graph.node_property("person", 0, "gremlin.pageRankVertexProgram.pageRank"),
        Value::Null
    );
}

#[cfg(feature = "duckdb")]
#[tokio::test]
#[ignore = "requires the production JVM classpath"]
async fn public_engine_executes_ir_and_respects_outer_rollback() {
    let mut engine = orchiddb::engine::GraphEngine::in_memory().unwrap();
    engine
        .gremlin("g.addV('person').property('name','caller')")
        .await
        .unwrap();
    engine.begin().unwrap();
    engine.gremlin("g.V().call('orchiddb.jvm',['script':\"current.property('name','changed'); current\"])").await.unwrap();
    engine.rollback().unwrap();
    let rows = engine.gremlin("g.V().values('name')").await.unwrap();
    assert_eq!(
        arrow::util::display::array_value_to_string(rows.returned.batch.column(0), 0).unwrap(),
        "caller"
    );
}

#[tokio::test]
#[ignore = "requires the production JVM classpath"]
async fn ordinary_gremlin_callbacks_use_the_same_engine_and_relational_executor() {
    use orchiddb::engine::{ExecutionBackend, GraphEngine};
    use orchiddb::language::gremlin::GremlinBinding;
    use std::collections::HashMap;
    let mut engine = GraphEngine::in_memory().unwrap();
    engine
        .gremlin("g.addV('person').property('name','alice').property('age',20)")
        .await
        .unwrap();
    engine
        .gremlin("g.addV('person').property('name','bob').property('age',30)")
        .await
        .unwrap();
    let bindings = HashMap::from([
        (
            "f".into(),
            GremlinBinding::Lambda("it.get().value('age') > 20".into()),
        ),
        (
            "m".into(),
            GremlinBinding::Lambda("it.get().value('name').toUpperCase()".into()),
        ),
        (
            "key".into(),
            GremlinBinding::Lambda("it.value('name').substring(0,1)".into()),
        ),
    ]);
    let output = engine
        .gremlin_with_bindings("g.V().filter(f).map(m)", &bindings)
        .await
        .unwrap();
    assert_eq!(output.backend, ExecutionBackend::DataFusion);
    assert_eq!(output.returned.batch.num_rows(), 1);
    let metadata = output.returned.batch.schema();
    assert!(metadata.metadata()["orchiddb.gremlin.typed_rows.v1"].contains("BOB"));
    let grouped = engine
        .gremlin_with_bindings("g.V().group().by(key).by(__.count())", &bindings)
        .await
        .unwrap();
    assert_eq!(grouped.returned.batch.num_rows(), 1);
    let inline = engine
        .gremlin("g.V().map(Lambda.function(\"it.get().value('name')\"))")
        .await
        .unwrap();
    assert_eq!(inline.returned.batch.num_rows(), 2);
    let bindings = HashMap::from([(
        "f".into(),
        GremlinBinding::Lambda("it.path('a').value('name')".into()),
    )]);
    let path = engine
        .gremlin_with_bindings("g.withPath().V().as('a').map(f)", &bindings)
        .await
        .unwrap();
    assert_eq!(path.returned.batch.num_rows(), 2);
    let bindings = HashMap::from([(
        "f".into(),
        GremlinBinding::Lambda(
            "it.get().property('name','lost'); throw new IllegalStateException('callback failure')"
                .into(),
        ),
    )]);
    assert!(
        engine
            .gremlin_with_bindings("g.V().map(f)", &bindings)
            .await
            .is_err()
    );
    assert_eq!(
        engine
            .gremlin("g.V().has('name','lost')")
            .await
            .unwrap()
            .returned
            .batch
            .num_rows(),
        0
    );
}

#[tokio::test]
#[ignore = "requires the production JVM classpath"]
async fn production_comparator_preserves_rows_and_graph_computer_uses_query_local_view() {
    use orchiddb::engine::GraphEngine;
    use orchiddb::language::gremlin::GremlinBinding;
    use std::collections::HashMap;
    let mut engine = GraphEngine::in_memory().unwrap();
    let bindings = HashMap::from([
        (
            "first".into(),
            GremlinBinding::Lambda("a,b -> a.substring(1,2).compareTo(b.substring(1,2))".into()),
        ),
        (
            "second".into(),
            GremlinBinding::Lambda("a,b -> b.substring(2,3).compareTo(a.substring(2,3))".into()),
        ),
    ]);
    let result=engine.gremlin_with_bindings("g.inject('ax1','az1','ax2').as('original').order().by(first).by(second).select('original')",&bindings).await.unwrap();
    let rows: serde_json::Value = serde_json::from_str(
        &result.returned.batch.schema().metadata()["orchiddb.gremlin.typed_rows.v1"],
    )
    .unwrap();
    assert_eq!(rows[0][0]["value"], "ax2");
    assert_eq!(rows[1][0]["value"], "ax1");
    assert_eq!(rows[2][0]["value"], "az1");
    engine.gremlin("g.addV('person').property('name','a').as('a').addV('person').property('name','b').addE('knows').from('a')").await.unwrap();
    assert_eq!(
        engine
            .gremlin("g.V()")
            .await
            .unwrap()
            .returned
            .batch
            .num_rows(),
        2
    );
    let connected=engine.gremlin("g.V().connectedComponent().with('~tinkerpop.connectedComponent.propertyName','component').values('component')").await.unwrap();
    assert_eq!(connected.returned.batch.num_rows(), 2);
    assert_eq!(
        engine
            .gremlin("g.V().has('component')")
            .await
            .unwrap()
            .returned
            .batch
            .num_rows(),
        0
    );
    let rank=engine.gremlin("g.V().pageRank(1.0).with('~tinkerpop.pageRank.times',1).with('~tinkerpop.pageRank.propertyName','rank').values('rank')").await.unwrap();
    assert_eq!(rank.returned.batch.num_rows(), 2);
    let shortest=engine.gremlin("g.V().has('name','a').shortestPath().with('~tinkerpop.shortestPath.target',__.has('name','b'))").await.unwrap();
    assert_eq!(shortest.returned.batch.num_rows(), 1);
}

#[tokio::test]
#[ignore = "requires the production JVM classpath"]
async fn correlated_callbacks_share_worker_and_preserve_typed_injection() {
    use orchiddb::language::gremlin::{GremlinBinding, semantics::GValue};
    use std::collections::HashMap;
    let mut engine = orchiddb::engine::GraphEngine::in_memory().unwrap();
    let bindings = HashMap::from([
        ("seed".into(), GremlinBinding::Value(GValue::Long(1))),
        (
            "increment".into(),
            GremlinBinding::Lambda("it.get() + 1L".into()),
        ),
    ]);
    let result = engine
        .gremlin_with_bindings(
            "g.inject(seed).repeat(__.map(increment)).times(20)",
            &bindings,
        )
        .await
        .unwrap();
    let rows: serde_json::Value = serde_json::from_str(
        &result.returned.batch.schema().metadata()["orchiddb.gremlin.typed_rows.v1"],
    )
    .unwrap();
    assert_eq!(rows[0][0]["value"], 21);
    assert_eq!(rows[0][0]["type"], "long");
}

#[tokio::test]
#[ignore = "requires the production JVM classpath"]
async fn sack_supplier_split_and_update_are_relational_jvm_operators() {
    use orchiddb::engine::{ExecutionBackend, GraphEngine};
    use orchiddb::language::gremlin::{GremlinBinding, semantics::GValue};
    use std::collections::HashMap;
    let mut engine=GraphEngine::in_memory().unwrap();
    engine.gremlin("g.addV('a').as('a').addV('b').as('b').addE('link').from('a').to('b')").await.unwrap();
    let bindings=HashMap::from([
        ("config".into(),GremlinBinding::Value(GValue::SackCallbacks {
            supplier:"{ -> 7 }".into(), split:Some("{ sack -> sack + 10 }".into()),
        })),
        ("update".into(),GremlinBinding::Lambda("sack, vertex -> sack + vertex.label().length()".into())),
    ]);
    let result=engine.gremlin_with_bindings("g.withSack(config).V().hasLabel('a').out().sack(update).sack()",&bindings).await.unwrap();
    assert_eq!(result.backend,ExecutionBackend::Hybrid);
    assert_eq!(result.returned.batch.num_rows(),1);
    assert_eq!(arrow::util::display::array_value_to_string(result.returned.batch.column(0),0).unwrap(),"18");
    let failed=HashMap::from([("update".into(),GremlinBinding::Lambda("sack, vertex -> vertex.property('lost',true); throw new IllegalStateException('sack callback failed')".into()))]);
    assert!(engine.gremlin_with_bindings("g.withSack(1).V().sack(update)",&failed).await.is_err());
    assert_eq!(engine.gremlin("g.V().has('lost')").await.unwrap().returned.batch.num_rows(),0);
}
