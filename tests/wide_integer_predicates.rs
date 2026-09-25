//! Wide-integer literal boundary regression tests.
//!
//! Pins the exact-comparison semantics for out-of-range integer literals in
//! scan predicates (`WHERE t.id = 170141183460469231731687303715884105727`).
//! The interpreter is the reference and compares through exact
//! `BigDecimal`/`BigInt` promotion, so a literal one below a stored power of
//! two must not match it. The DuckDB path historically round-tripped the
//! literal through `f64` (see `docs/sql_islands_completion.md` section 1),
//! turning `i128::MAX` into `2^127` and matching a stored `2^127.0`.
//!
//! Both paths test exact equality without coercing the literal to a rounded
//! float. DuckDB tests require the default `duckdb` feature.

use std::sync::Arc;

use arrow::array::{ArrayRef, Float64Array};

use orchiddb::ir::catalog::{PropertyGraph, nodes_from_columns};
use orchiddb::ir::interpreter::execute;
use orchiddb::language::cypher::parser::parse_query;
use orchiddb::language::cypher::planner::CypherPlanner;

const I128_MAX: &str = "170141183460469231731687303715884105727";
const I128_MAX_PLUS_ONE: &str = "170141183460469231731687303715884105728";
const U128_MAX: &str = "340282366920938463463374607431768211455";
const U128_MAX_PLUS_ONE: &str = "340282366920938463463374607431768211456";

/// Two nodes whose `id` property is a `Float64` holding the values one above
/// the signed and unsigned 128-bit maxima (`2^127` and `2^128`). These are
/// exactly the values a boundary literal round-trips to when coerced through
/// `f64`, so a query for `i128::MAX` / `u128::MAX` must not match either.
fn boundary_graph() -> PropertyGraph {
    let ids: ArrayRef = Arc::new(Float64Array::from(vec![2_f64.powi(127), 2_f64.powi(128)]));
    let mut graph = PropertyGraph::new();
    graph.add_nodes(nodes_from_columns("test", vec![("id", ids)]));
    graph
}

fn interpreter_rows(graph: &PropertyGraph, query: &str) -> Vec<String> {
    let parsed = parse_query(query).expect("parse");
    let plan = CypherPlanner::new().plan(&parsed).expect("plan");
    let returned = execute(&plan, graph).expect("run interpreter");
    let batch = returned.batch;
    (0..batch.num_rows())
        .map(|row| {
            (0..batch.num_columns())
                .map(|col| {
                    arrow::util::display::array_value_to_string(batch.column(col), row)
                        .unwrap_or_default()
                })
                .collect::<Vec<_>>()
                .join("|")
        })
        .collect()
}

#[test]
fn interpreter_i128_max_literal_does_not_false_match() {
    let graph = boundary_graph();
    let query = format!("MATCH (t:test) WHERE t.id = {I128_MAX} RETURN t.id");
    assert!(
        interpreter_rows(&graph, &query).is_empty(),
        "`i128::MAX` literal must not match a stored `2^127`"
    );
}

#[test]
fn interpreter_u128_max_literal_does_not_false_match() {
    let graph = boundary_graph();
    let query = format!("MATCH (t:test) WHERE t.id = {U128_MAX} RETURN t.id");
    assert!(
        interpreter_rows(&graph, &query).is_empty(),
        "`u128::MAX` literal must not match a stored `2^128`"
    );
}

#[test]
fn interpreter_exact_boundary_values_still_match() {
    let graph = boundary_graph();
    let at_max = format!("MATCH (t:test) WHERE t.id = {I128_MAX_PLUS_ONE} RETURN t.id");
    assert_eq!(
        interpreter_rows(&graph, &at_max).len(),
        1,
        "`2^127` literal must match the stored `2^127`"
    );
    let at_umax = format!("MATCH (t:test) WHERE t.id = {U128_MAX_PLUS_ONE} RETURN t.id");
    assert_eq!(
        interpreter_rows(&graph, &at_umax).len(),
        1,
        "`2^128` literal must match the stored `2^128`"
    );
}

#[cfg(feature = "duckdb")]
async fn duckdb_rows(graph: &PropertyGraph, query: &str) -> Vec<String> {
    use orchiddb::ir::rel::RelBackend;
    use orchiddb::ir::rel::sql::{self, DuckDbExecutor, SqlDialect};

    let parsed = parse_query(query).expect("parse");
    let plan = CypherPlanner::new().plan(&parsed).expect("plan");
    let backend = RelBackend::new();
    let lowered = backend.lower(&plan, graph).expect("lower");
    let prepared = sql::prepare(&lowered, SqlDialect::DuckDb)
        .await
        .expect("prepare sql");
    let mut executor = DuckDbExecutor::new();
    let from_duckdb = sql::execute_prepared(&mut executor, &prepared)
        .unwrap_or_else(|err| panic!("duckdb execute: {err}\nquery: {}", prepared.query));
    (0..from_duckdb.batch.num_rows())
        .map(|row| {
            (0..from_duckdb.batch.num_columns())
                .map(|col| {
                    arrow::util::display::array_value_to_string(from_duckdb.batch.column(col), row)
                        .unwrap_or_default()
                })
                .collect::<Vec<_>>()
                .join("|")
        })
        .collect()
}

#[cfg(feature = "duckdb")]
#[tokio::test]
async fn duckdb_scan_predicate_compares_wide_literals_exactly() {
    let graph = boundary_graph();

    let at_max = format!("MATCH (t:test) WHERE t.id = {I128_MAX} RETURN t.id");
    assert!(
        duckdb_rows(&graph, &at_max).await.is_empty(),
        "`i128::MAX` literal must not match a stored `2^127`"
    );
    let at_umax = format!("MATCH (t:test) WHERE t.id = {U128_MAX} RETURN t.id");
    assert!(
        duckdb_rows(&graph, &at_umax).await.is_empty(),
        "`u128::MAX` literal must not match a stored `2^128`"
    );
    let exact = format!("MATCH (t:test) WHERE t.id = {I128_MAX_PLUS_ONE} RETURN t.id");
    assert_eq!(
        duckdb_rows(&graph, &exact).await.len(),
        1,
        "`2^127` literal must match the stored `2^127`"
    );
}
