#![cfg(feature = "duckdb")]

use std::sync::Arc;

use arrow::{
    array::{Int64Array, RecordBatch},
    datatypes::{DataType, Field, Schema},
};
use new_graph::{
    ir::{
        catalog::{NodeTable, PropertyGraph},
        rel::{
            RelBackend,
            sql::{self, DuckDbExecutor, SqlDialect},
        },
    },
    language::cypher::{parse_query, planner::CypherPlanner},
};

fn graph() -> PropertyGraph {
    let batch = RecordBatch::try_new(
        Arc::new(Schema::new(vec![Field::new("x", DataType::Int64, true)])),
        vec![Arc::new(Int64Array::from(vec![
            Some(1),
            Some(2),
            Some(2),
            Some(7),
            None,
        ]))],
    )
    .unwrap();
    let mut graph = PropertyGraph::new();
    graph.add_nodes(NodeTable {
        label: "P".into(),
        batch,
    });
    graph
}

async fn rows(query: &str) -> Vec<String> {
    let parsed = parse_query(query).unwrap();
    let plan = CypherPlanner::new().plan(&parsed).unwrap();
    let lowered = RelBackend::new().lower(&plan, &graph()).unwrap();
    let prepared = sql::prepare(&lowered, SqlDialect::DuckDb).await.unwrap();
    let result = sql::execute_prepared(&mut DuckDbExecutor::new(), &prepared)
        .unwrap_or_else(|err| panic!("{err}\n{}", prepared.query));
    let mut rows = (0..result.batch.num_rows())
        .map(|row| {
            (0..result.batch.num_columns())
                .map(|col| {
                    arrow::util::display::array_value_to_string(
                        result.batch.column(col).as_ref(),
                        row,
                    )
                    .unwrap()
                })
                .collect::<Vec<_>>()
                .join("|")
        })
        .collect::<Vec<_>>();
    rows.sort();
    rows
}

#[tokio::test]
async fn native_aggregates_support_multiple_arguments_and_scalar_composition() {
    assert_eq!(
        rows("MATCH (p:P) RETURN median(p.x), quantile_cont(p.x, 0.5), arg_max(p.x, -p.x)").await,
        ["2.0|2.0|1"]
    );
    assert_eq!(rows("MATCH (p:P) RETURN median(p.x) + 1").await, ["3.0"]);
}

#[tokio::test]
async fn native_aggregates_preserve_distinct_grouping_and_empty_input() {
    assert_eq!(rows("MATCH (p:P) RETURN list(DISTINCT p.x)").await.len(), 1);
    assert_eq!(
        rows("MATCH (p:P) RETURN p.x AS x, median(p.x) AS m").await,
        ["1|1.0", "2|2.0", "7|7.0", "|"]
    );
    assert_eq!(
        rows("MATCH (p:P) WHERE p.x > 100 RETURN median(p.x)").await,
        [""]
    );
    assert_eq!(
        rows("MATCH (p:P) RETURN product(DISTINCT p.x)").await,
        ["14.0"]
    );
}

#[tokio::test]
async fn standard_statistical_aggregates_map_to_duckdb() {
    assert_eq!(
        rows("MATCH (p:P) RETURN percentileCont(p.x, 0.5), percentileDisc(p.x, 0.5)").await,
        ["2.0|2"]
    );
    assert_eq!(
        rows("MATCH (p:P) WHERE p.x > 100 RETURN stdev(p.x), stdevp(p.x)").await,
        ["0.0|0.0"]
    );
    assert_eq!(
        rows("MATCH (p:P) WHERE p.x = 7 RETURN stdev(p.x), stdevp(p.x)").await,
        ["0.0|0.0"]
    );
}

#[test]
fn native_nested_aggregates_are_rejected() {
    let parsed = parse_query("MATCH (p:P) RETURN median(sum(p.x))").unwrap();
    assert!(CypherPlanner::new().plan(&parsed).is_err());
}

#[test]
fn native_aggregate_invalid_arity_is_rejected_during_binding() {
    let parsed = parse_query("MATCH (p:P) RETURN quantile_cont(p.x)").unwrap();
    let plan = CypherPlanner::new().plan(&parsed).unwrap();
    assert!(RelBackend::new().lower(&plan, &graph()).is_err());
}

#[test]
fn interpreter_reports_native_aggregate_requirement_even_on_empty_input() {
    let parsed = parse_query("MATCH (p:P) WHERE p.x > 100 RETURN median(p.x)").unwrap();
    let plan = CypherPlanner::new().plan(&parsed).unwrap();
    let error = new_graph::ir::interpreter::execute_rows(&plan, &graph()).unwrap_err();
    assert!(
        error.to_string().contains("require relational execution"),
        "{error}"
    );
}

#[tokio::test]
async fn zero_argument_native_aggregate_preserves_unwind_rows() {
    assert_eq!(
        rows("UNWIND [1, 2, 3] AS x RETURN count_star()").await,
        ["3"]
    );
}

#[test]
fn every_engine_expression_function_name_fits_existing_grammar() {
    use new_graph::ir::functions::{DuckDbCatalog, FunctionKind};
    let catalog = DuckDbCatalog::new().unwrap();
    let mut checked = std::collections::BTreeSet::new();
    for overload in catalog.functions().filter(|f| {
        matches!(
            f.kind,
            FunctionKind::Scalar | FunctionKind::Aggregate | FunctionKind::Macro
        )
    }) {
        if !checked.insert(&overload.name) {
            continue;
        }
        let escaped = overload.name.replace('`', "``");
        let args = vec!["NULL"; overload.parameter_types.len()].join(", ");
        let query = format!("RETURN `{escaped}`({args})");
        parse_query(&query).unwrap_or_else(|error| panic!("{query}: {error}"));
    }
    assert!(
        checked.len() > 500,
        "unexpectedly small engine expression catalog: {}",
        checked.len()
    );
}

#[tokio::test]
async fn native_aggregate_accepts_list_parameters() {
    assert_eq!(
        rows("MATCH (p:P) RETURN quantile_cont(p.x, [0.25, 0.5])").await,
        ["[1.75, 2.0]"]
    );
}
