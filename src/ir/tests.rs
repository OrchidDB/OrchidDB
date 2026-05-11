//! Interpreter tests built around a small "modern" property graph that
//! mirrors the TinkerPop tutorial dataset. Tests cover scans, filters,
//! expansions, projections, distinct, aggregates, sort/limit, optional /
//! semi / anti / scalar apply, unwind, repeat, coalesce, choose, select,
//! and Gremlin bulk semantics.

use std::sync::Arc;

use arrow::array::{Array, ArrayRef, Float64Array, Int64Array, StringArray};
use arrow::datatypes::DataType;

use super::bridge::{cypher as cb, gremlin as gb};
use super::expr::{AggCall, AggKind, BinaryOp, IrExpr, Lit, StringOp};
use super::interpreter::{Row, execute, execute_rows};
use super::plan::{
    ApplyKind, ChooseArm, ChooseSelector, ChooseUnmatched, Direction, DistinctBulk, DistinctMode,
    EmitMode, GraphPlan, JoinKind, LabelExpr, Length, Node, NullsOrder, PathMaterialization,
    PathObjects, PathUpdate, ProjectErrorPolicy, ProjectMode, ProjectionItem, Slice, SortDir,
    SortKey, TargetMode, UnionAlign,
};
use super::policy::{
    GraphPlanPolicy, MatchMode, OptionalMissing, PathMode, PropertyMissing, ProviderFeature,
};
use super::value::Value;
use super::{
    catalog::{PropertyGraph, edges_from_columns, nodes_from_columns},
    df::{GraphValues, downcast_graph_ir, from_logical_plan, to_logical_plan},
    policy::ResultForm,
};

// ---------------- fixtures ----------------

fn modern_graph() -> PropertyGraph {
    // Persons: marko(0,29), vadas(1,27), josh(2,32), peter(3,35)
    let names: ArrayRef = Arc::new(StringArray::from(vec!["marko", "vadas", "josh", "peter"]));
    let ages: ArrayRef = Arc::new(Int64Array::from(vec![29, 27, 32, 35]));
    let countries: ArrayRef = Arc::new(StringArray::from(vec!["us", "us", "uk", "uk"]));
    let person = nodes_from_columns(
        "Person",
        vec![("name", names), ("age", ages), ("country", countries)],
    );

    // Software: lop(0,java), ripple(1,java)
    let titles: ArrayRef = Arc::new(StringArray::from(vec!["lop", "ripple"]));
    let langs: ArrayRef = Arc::new(StringArray::from(vec!["java", "java"]));
    let software = nodes_from_columns("Software", vec![("name", titles), ("lang", langs)]);

    let mut graph = PropertyGraph::new();
    graph.add_nodes(person);
    graph.add_nodes(software);

    // KNOWS edges between persons:
    //   marko --knows(0.5)--> vadas
    //   marko --knows(1.0)--> josh
    let knows_weight: ArrayRef = Arc::new(Float64Array::from(vec![0.5, 1.0]));
    let knows_since: ArrayRef = Arc::new(Int64Array::from(vec![2010, 2015]));
    let knows = edges_from_columns(
        "KNOWS",
        "Person",
        "Person",
        vec![0, 0],
        vec![1, 2],
        vec![("weight", knows_weight), ("since", knows_since)],
    );
    graph.add_edges(knows).unwrap();

    // CREATED edges person → software:
    //   marko -> lop, josh -> lop, josh -> ripple, peter -> lop
    let created_weight: ArrayRef = Arc::new(Float64Array::from(vec![0.4, 0.4, 1.0, 0.2]));
    let created = edges_from_columns(
        "CREATED",
        "Person",
        "Software",
        vec![0, 2, 2, 3],
        vec![0, 0, 1, 0],
        vec![("weight", created_weight)],
    );
    graph.add_edges(created).unwrap();

    graph
}

fn cypher_plan(root: Node) -> GraphPlan {
    GraphPlan::new(GraphPlanPolicy::cypher(), root)
}

fn gremlin_plan(root: Node) -> GraphPlan {
    GraphPlan::new(GraphPlanPolicy::gremlin(), root)
}

fn return_node(input: Node, fields: Vec<&str>) -> Node {
    Node::GraphReturn {
        fields: fields.into_iter().map(String::from).collect(),
        result_form: ResultForm::RowSet,
        input: input.boxed(),
    }
}

fn property(binding: &str, name: &str) -> IrExpr {
    IrExpr::property(binding, name, PropertyMissing::NullOnMissing)
}

fn drop_unproductive(binding: &str, name: &str) -> IrExpr {
    IrExpr::property(binding, name, PropertyMissing::DropUnproductive)
}
