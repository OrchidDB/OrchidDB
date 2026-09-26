//! Regression coverage for SQL regions and native kernels in the DataFusion DAG.
use arrow::array::{ArrayRef, Int64Array, StringArray};
use orchiddb::ir::catalog::{PropertyGraph, edges_from_columns, nodes_from_columns};
use orchiddb::ir::rel::runtime::execute_rows_with_jvm;
use orchiddb::ir::{Node, Value};
use orchiddb::language::cypher::parser::parse_query;
use orchiddb::language::cypher::planner::CypherPlanner;
use std::sync::Arc;
/// Two people, one `knows` edge between them.
fn fixture() -> PropertyGraph {
    let names: ArrayRef = Arc::new(StringArray::from(vec!["alice", "bob"]));
    let ages: ArrayRef = Arc::new(Int64Array::from(vec![30, 40]));
    let mut graph = PropertyGraph::new();
    graph.add_nodes(nodes_from_columns(
        "person",
        vec![("name", names), ("age", ages)],
    ));
    graph
        .add_edges(edges_from_columns(
            "knows",
            "person",
            "person",
            vec![0],
            vec![1],
            Vec::new(),
        ))
        .unwrap();
    graph
}

async fn rows(graph: &PropertyGraph, query: &str) -> Vec<Vec<Value>> {
    let plan = CypherPlanner::new()
        .plan(&parse_query(query).unwrap())
        .unwrap();
    let Node::GraphReturn { fields, .. } = plan.root.as_ref() else {
        panic!("expected return");
    };
    let (rows, stats) = execute_rows_with_jvm(&plan, graph, Default::default())
        .await
        .unwrap();
    assert!(!stats.physical_plan.is_empty());
    let mut result: Vec<_> = rows
        .into_iter()
        .flat_map(|r| {
            let cells: Vec<_> = fields.iter().map(|f| r.bindings[f].clone()).collect();
            std::iter::repeat_n(cells, r.bulk as usize)
        })
        .collect();
    result.sort_by_key(|r| format!("{r:?}"));
    result
}
fn text(s: &str) -> Value {
    Value::String(s.into())
}
#[tokio::test]
async fn filtered_scan_has_expected_rows() {
    assert_eq!(
        rows(
            &fixture(),
            "MATCH (p:person) WHERE p.age > 35 RETURN p.name"
        )
        .await,
        vec![vec![text("bob")]]
    );
}
#[tokio::test]
async fn result_boundary_preserves_language_metadata() {
    let plan = CypherPlanner::new()
        .plan(&parse_query("MATCH (p:person) RETURN p.name").unwrap())
        .unwrap();
    let (result, stats) = orchiddb::ir::rel::runtime::execute(&plan, &fixture(), None)
        .await
        .unwrap();
    assert_eq!(result.batch.num_rows(), 2);
    assert!(
        result
            .batch
            .schema()
            .metadata()
            .contains_key("orchiddb.cypher.typed_rows.v1")
    );
    assert!(!stats.physical_plan.is_empty());
}
#[tokio::test]
async fn expand_preserves_source_and_destination() {
    assert_eq!(
        rows(
            &fixture(),
            "MATCH (a:person)-[:knows]->(b:person) RETURN a.name,b.name"
        )
        .await,
        vec![vec![text("alice"), text("bob")]]
    );
}
#[tokio::test]
async fn dynamic_list_kernel_composes_with_scan() {
    assert_eq!(
        rows(&fixture(), "MATCH (p:person) RETURN list_sort([1,p.age])").await,
        vec![
            vec![Value::List(vec![Value::Int(1), Value::Int(30)])],
            vec![Value::List(vec![Value::Int(1), Value::Int(40)])]
        ]
    );
}
#[tokio::test]
async fn mutations_persist_through_the_dag() {
    let graph = fixture();
    assert_eq!(
        rows(&graph, "CREATE (n:person {id:99}) RETURN n.id").await,
        vec![vec![Value::Int(99)]]
    );
    assert_eq!(graph.node_ids("person").unwrap().len(), 3);
}
#[tokio::test]
async fn collected_strings_remain_a_typed_list() {
    assert_eq!(
        rows(&fixture(), "MATCH (p:person) RETURN collect(p.name)").await,
        vec![vec![Value::List(vec![text("alice"), text("bob")])]]
    );
}
#[tokio::test]
async fn collected_integer_and_group_columns_are_not_null() {
    let graph = fixture();
    assert_eq!(
        rows(&graph, "MATCH (p:person) RETURN collect(p.age)").await,
        vec![vec![Value::List(vec![Value::Int(30), Value::Int(40)])]]
    );
    assert_eq!(
        rows(&graph, "MATCH (p:person) RETURN p.age,collect(p.name)").await,
        vec![
            vec![Value::Int(30), Value::List(vec![text("alice")])],
            vec![Value::Int(40), Value::List(vec![text("bob")])]
        ]
    );
    assert_eq!(
        rows(
            &graph,
            "MATCH (a:person)-[:knows]->(b:person) RETURN a.name,collect(b.age)"
        )
        .await,
        vec![vec![text("alice"), Value::List(vec![Value::Int(40)])]]
    );
}
#[tokio::test]
async fn star_projection_retains_properties() {
    let result = rows(&fixture(), "MATCH (p:person) RETURN p.*").await;
    let expected = [("alice", 30), ("bob", 40)]
        .into_iter()
        .map(|(name, age)| {
            vec![Value::Map(
                [
                    (
                        "__orchiddb_struct_order".into(),
                        Value::List(vec![text("name"), text("age")]),
                    ),
                    ("age".into(), Value::Long(age)),
                    ("name".into(), text(name)),
                ]
                .into(),
            )]
        })
        .collect::<Vec<_>>();
    assert_eq!(result, expected);
}
#[tokio::test]
async fn two_projected_columns_preserve_types() {
    assert_eq!(
        rows(
            &fixture(),
            "MATCH (p:person) WHERE p.age >25 RETURN p.name,p.age"
        )
        .await,
        vec![
            vec![text("alice"), Value::Long(30)],
            vec![text("bob"), Value::Long(40)]
        ]
    );
}
#[tokio::test]
async fn repeated_execution_does_not_reuse_another_graph() {
    let graph = fixture();
    assert_eq!(
        rows(&graph, "MATCH (p:person) RETURN count(p)").await,
        vec![vec![Value::Long(2)]]
    );
    graph.insert_node("person", Default::default());
    assert_eq!(
        rows(&graph, "MATCH (p:person) RETURN count(p)").await,
        vec![vec![Value::Long(3)]]
    );
}
#[tokio::test]
async fn native_projection_errors_propagate() {
    let plan = CypherPlanner::new()
        .plan(&parse_query("RETURN 1 / 0").unwrap())
        .unwrap();
    let result = orchiddb::ir::rel::runtime::execute(&plan, &fixture(), None).await;
    assert!(result.is_err());
}
