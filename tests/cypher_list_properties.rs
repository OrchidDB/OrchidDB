#![cfg(feature = "duckdb")]

use arrow::{
    array::{ArrayRef, RecordBatch, StringArray},
    datatypes::{DataType, Field, Schema},
};
use orchiddb::ir::{
    catalog::{NodeTable, PropertyGraph},
    rel::{
        RelBackend,
        sql::{self, DuckDbExecutor, SqlDialect},
    },
    value::Value,
};
use orchiddb::language::cypher::{parse_query, planner::CypherPlanner};
use std::{collections::HashMap, sync::Arc};

fn graph(values: Vec<Value>) -> PropertyGraph {
    let field = Field::new("items", DataType::Utf8, true).with_metadata(HashMap::from([(
        "orchiddb.value_type".into(),
        "value".into(),
    )]));
    let values: ArrayRef = Arc::new(StringArray::from(
        values.iter().map(|v| format!("{v:?}")).collect::<Vec<_>>(),
    ));
    let batch = RecordBatch::try_new(Arc::new(Schema::new(vec![field])), vec![values]).unwrap();
    let mut graph = PropertyGraph::new();
    graph.add_nodes(NodeTable {
        label: "P".into(),
        batch,
    });
    graph
}

async fn rows(graph: &PropertyGraph, query: &str) -> Vec<String> {
    let parsed = parse_query(query).unwrap();
    let plan = CypherPlanner::new().plan(&parsed).unwrap();
    let lowered = RelBackend::new().lower(&plan, graph).unwrap();
    let prepared = sql::prepare(&lowered, SqlDialect::DuckDb).await.unwrap();
    let result = sql::execute_prepared(&mut DuckDbExecutor::new(), &prepared)
        .unwrap_or_else(|err| panic!("{err}\n{}", prepared.query));
    let batch = result.batch;
    let mut rows = (0..batch.num_rows())
        .map(|row| {
            (0..batch.num_columns())
                .map(|col| {
                    arrow::util::display::array_value_to_string(batch.column(col).as_ref(), row)
                        .unwrap()
                })
                .collect::<Vec<_>>()
                .join("|")
        })
        .collect::<Vec<_>>();
    rows.sort();
    rows
}

async fn hybrid_rows(graph: &PropertyGraph, query: &str) -> Vec<String> {
    let parsed = parse_query(query).unwrap();
    let plan = CypherPlanner::new().plan(&parsed).unwrap();
    let (returned, _) = orchiddb::ir::exec::execute_with_islands(
        &plan, graph, &RelBackend::new(), &orchiddb::ir::exec::SqlTarget::duckdb(),
    ).await.unwrap();
    let mut rows = (0..returned.batch.num_rows()).map(|row| {
        arrow::util::display::array_value_to_string(returned.batch.column(0).as_ref(), row).unwrap()
    }).collect::<Vec<_>>();
    rows.sort();
    rows
}

#[tokio::test]
async fn encoded_list_property_uses_elements_for_size_subscript_and_unwind() {
    let graph = graph(vec![
        Value::List(vec![Value::Int(10), Value::Int(20)]),
        Value::List(vec![]),
    ]);
    assert_eq!(
        rows(&graph, "MATCH (p:P) RETURN size(p.items)").await,
        ["0", "2"]
    );
    assert_eq!(
        rows(&graph, "MATCH (p:P) UNWIND p.items AS x RETURN x").await,
        ["10", "20"]
    );
    assert_eq!(
        rows(&graph, "MATCH (p:P) RETURN p.items[0]").await,
        ["", "10"]
    );
    assert_eq!(rows(&graph, "MATCH (p:P) RETURN p.items[1]").await, ["", "20"]);
    assert_eq!(
        rows(&graph, "MATCH (p:P) RETURN list_append(p.items, 30)").await,
        ["[10, 20, 30]", "[30]"]
    );
}

#[tokio::test]
async fn encoded_list_quantifier_handles_empty_and_null_elements() {
    let graph = graph(vec![
        Value::List(vec![Value::Int(10), Value::Null]),
        Value::List(vec![]),
        Value::Null,
        Value::List(vec![Value::Int(10), Value::Null]),
    ]);
    for (function, expected) in [
        ("any", vec!["", "false", "true", "true"]),
        ("all", vec!["", "", "", "true"]),
        ("none", vec!["", "false", "false", "true"]),
        ("single", vec!["", "", "", "false"]),
    ] {
        assert_eq!(
            hybrid_rows(
                &graph,
                &format!("MATCH (p:P) RETURN {function}(x IN p.items WHERE x = 10)")
            )
            .await,
            expected,
            "{function}"
        );
    }
}

#[test]
fn heterogeneous_encoded_list_does_not_become_text_subscript() {
    let graph = graph(vec![Value::List(vec![
        Value::Int(10),
        Value::String("x".into()),
    ])]);
    let parsed = parse_query("MATCH (p:P) RETURN p.items[1]").unwrap();
    let plan = CypherPlanner::new().plan(&parsed).unwrap();
    let error = RelBackend::new().lower(&plan, &graph).unwrap_err();
    assert!(
        error.to_string().contains("uniform native list type"),
        "{error}"
    );
}

#[tokio::test]
async fn encoded_nested_and_string_lists_keep_native_element_types() {
    let nested = graph(vec![Value::List(vec![
        Value::List(vec![]),
        Value::List(vec![Value::Int(7)]),
    ])]);
    assert_eq!(
        rows(&nested, "MATCH (p:P) UNWIND p.items AS x RETURN size(x)").await,
        ["0", "1"]
    );
    let strings = graph(vec![
        Value::List(vec![
            Value::String("hello".into()),
            Value::String("world".into()),
        ]),
        Value::Null,
    ]);
    assert_eq!(
        rows(
            &strings,
            "MATCH (p:P) RETURN list_contains(p.items, 'world')"
        )
        .await,
        ["", "true"]
    );
    assert_eq!(
        rows(&strings, "MATCH (p:P) RETURN list_slice(p.items, 2, 2)").await,
        ["", "[world]"]
    );
}

#[tokio::test]
async fn dynamic_string_slice_clamps_unicode_bounds() {
    assert_eq!(
        rows(&PropertyGraph::new(), "WITH 'aé猫z' AS text RETURN list_slice(text, 2, -2), list_slice(text, -99, 99), list_slice(text, 9, 10)").await,
        ["é猫|aé猫z|"]
    );
}
