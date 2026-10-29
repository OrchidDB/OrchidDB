#![cfg(feature = "duckdb")]
use orchiddb::ir::{catalog::PropertyGraph, procedures::{ProcedureField, ProcedureSignature, TableProcedure}, rel::{RelBackend, sql::{DuckDbExecutor, execute_lowered_sql}}, Value};
use std::collections::BTreeMap;

fn field(name: &str, kind: &str) -> ProcedureField {
    ProcedureField { name: name.into(), type_name: kind.into(), nullable: true }
}
fn table(inputs: Vec<ProcedureField>, outputs: Vec<ProcedureField>, rows: Vec<Vec<Value>>) -> TableProcedure {
    TableProcedure { signature: ProcedureSignature { inputs, outputs }, rows }
}
async fn sql_rows(query: &str, procedure: TableProcedure) -> Vec<Vec<String>> {
    let mut graph = PropertyGraph::new();
    graph.register_table_procedure("test.lookup".into(), procedure.clone()).unwrap();
    let catalog = BTreeMap::from([("test.lookup".into(), procedure)]);
    let plan = orchiddb::language::cypher::preparation::prepare(query, &BTreeMap::new(), Some(&catalog)).unwrap();
    let lowered = RelBackend::new().with_language_functions(true).lower(&plan, &graph).unwrap();
    let mut executor = DuckDbExecutor::default();
    let result = execute_lowered_sql(&mut executor, &lowered).await.unwrap_or_else(|error| panic!("{query}: {error}\n{}\n{}", lowered.plan.display_indent(), orchiddb::ir::rel::sql::unparse(&lowered, orchiddb::ir::rel::sql::SqlDialect::DuckDb).unwrap()));
    let batch = &result.batch;
    (0..batch.num_rows()).map(|row| batch.columns().iter().map(|column| {
        arrow::util::display::array_value_to_string(column, row).unwrap()
    }).collect()).collect()
}

#[tokio::test]
async fn immutable_table_calls_lower_without_native_functions() {
    let fixture = table(vec![field("id", "NUMBER")], vec![field("a", "STRING"), field("b", "INTEGER")], vec![
        vec![Value::Int(42), Value::String("z".into()), Value::Int(1)],
        vec![Value::Float(42.0), Value::String("a".into()), Value::Int(2)],
        vec![Value::Null, Value::String("null".into()), Value::Int(3)],
        vec![Value::Int(42), Value::String("a".into()), Value::Int(2)],
    ]);
    let rows = sql_rows("CALL test.lookup(42) YIELD a AS b,b AS a RETURN a,b ORDER BY a,b", fixture.clone()).await;
    assert_eq!(rows, vec![vec!["1", "z"], vec!["2", "a"], vec!["2", "a"]]);
    assert_eq!(sql_rows("CALL test.lookup(null) YIELD a RETURN a", fixture.clone()).await, vec![vec!["null"]]);
    assert!(sql_rows("CALL test.lookup(99) YIELD a RETURN a", fixture).await.is_empty());
}

#[tokio::test]
async fn void_and_duplicate_upstream_rows_keep_cardinality() {
    let void = table(vec![], vec![], vec![]);
    assert_eq!(sql_rows("UNWIND [1,1,2] AS x CALL test.lookup() RETURN x ORDER BY x", void).await,
        vec![vec!["1"],vec!["1"],vec!["2"]]);
    let producer = table(vec![], vec![field("a", "STRING")], vec![vec![Value::String("a".into())], vec![Value::String("a".into())]]);
    assert_eq!(sql_rows("CALL test.lookup() YIELD a RETURN count(*) AS n", producer).await,
        vec![vec!["2"]]);
}

#[test]
fn row_dependent_invalid_and_volatile_calls_keep_capability_boundary() {
    let fixture = table(vec![field("id", "INTEGER")], vec![field("a", "STRING")], vec![]);
    let mut graph = PropertyGraph::new();
    graph.register_table_procedure("test.lookup".into(), fixture.clone()).unwrap();
    let catalog = BTreeMap::from([("test.lookup".into(), fixture)]);
    for query in ["UNWIND [1,2] AS x CALL test.lookup(x) YIELD a RETURN a", "CALL test.lookup(rand()) YIELD a RETURN a"] {
        let plan = orchiddb::language::cypher::preparation::prepare(query, &BTreeMap::new(), Some(&catalog)).unwrap();
        assert!(RelBackend::new().with_language_functions(true).lower(&plan,&graph).is_err(), "{query}");
    }
}

#[tokio::test]
async fn singleton_composed_calls_preserve_outer_fields_and_registration_order() {
    let fixture = table(vec![], vec![field("a", "STRING")], vec![
        vec![Value::String("z".into())], vec![Value::String("a".into())], vec![Value::String("z".into())],
    ]);
    assert_eq!(sql_rows("CALL test.lookup() YIELD a WITH count(*) AS c CALL test.lookup() YIELD a RETURN c,a", fixture.clone()).await,
        vec![vec!["3", "z"],vec!["3", "a"],vec!["3", "z"]]);
    assert_eq!(sql_rows("UNWIND [] AS x WITH count(*) AS c CALL test.lookup() YIELD a RETURN c,a LIMIT 2", fixture).await,
        vec![vec!["0", "z"],vec!["0", "a"]]);
}

#[test]
fn composed_empty_call_keeps_throwing_and_volatile_aggregate_sources_native() {
    let fixture = table(vec![], vec![field("a", "STRING")], vec![]);
    let mut graph = PropertyGraph::new();
    graph.register_table_procedure("test.lookup".into(), fixture.clone()).unwrap();
    let catalog = BTreeMap::from([("test.lookup".into(), fixture)]);
    for query in [
        "UNWIND [0,1] AS x WITH sum(1/x) AS c CALL test.lookup() YIELD a RETURN c,a",
        "UNWIND [0,1] AS x WITH rand() AS r WITH count(r) AS c CALL test.lookup() YIELD a RETURN c,a",
        "UNWIND [0,1] AS x WITH 1/x AS k,count(*) AS n WITH count(*) AS c CALL test.lookup() YIELD a RETURN c,a",
        "UNWIND [0,1] AS x WITH rand() AS k,count(*) AS n WITH count(*) AS c CALL test.lookup() YIELD a RETURN c,a",
    ] {
        let plan = orchiddb::language::cypher::preparation::prepare(query, &BTreeMap::new(), Some(&catalog)).unwrap();
        assert!(RelBackend::new().with_language_functions(true).lower(&plan, &graph).is_err(), "{query}");
    }
}
