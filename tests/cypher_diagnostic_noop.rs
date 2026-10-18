#[path = "../tools/diagnostics/cypher_noop.rs"]
mod cypher_noop;
use orchiddb::{ir::{plan::ProcedureMode, procedures::{ProcedureCatalog, ProcedureField, ProcedureSignature, TableProcedure}}, language::cypher::preparation::prepare};
use std::collections::BTreeMap;

fn catalog() -> ProcedureCatalog {
    BTreeMap::from([("test.noop".into(), TableProcedure {
        signature: ProcedureSignature { inputs: vec![], outputs: vec![] }, rows: vec![],
    })])
}

#[test]
fn standalone_void_procedure_exports_no_execution_contract() {
    let catalog = catalog();
    for query in ["CALL test.noop", "CALL test.noop()"] {
        let plan = prepare(query, &BTreeMap::new(), Some(&catalog)).unwrap();
        assert!(cypher_noop::is_noop(&plan, &catalog), "{query}");
    }
    let output = cypher_noop::output();
    assert_eq!(output["execution_kind"], "no_op");
    assert!(output["sql"].is_null());
    assert_eq!(output["requires_sql_execution"], false);
    assert_eq!(output["result_rows"], serde_json::json!([]));
}

#[test]
fn upstream_rows_and_projections_are_not_noops() {
    let catalog = catalog();
    for query in [
        "UNWIND [1,1,2] AS x CALL test.noop() RETURN x",
        "CALL test.noop() RETURN 1 AS x",
        "RETURN 1 AS x",
    ] {
        let plan = prepare(query, &BTreeMap::new(), Some(&catalog)).unwrap();
        assert!(!cypher_noop::is_noop(&plan, &catalog), "{query}");
    }
}

#[test]
fn argument_evaluation_and_output_procedures_require_execution() {
    let mut catalog = catalog();
    catalog.insert("test.arg".into(), TableProcedure {
        signature: ProcedureSignature { inputs: vec![ProcedureField {
            name: "x".into(), type_name: "ANY".into(), nullable: true,
        }], outputs: vec![] }, rows: vec![],
    });
    catalog.insert("test.output".into(), TableProcedure {
        signature: ProcedureSignature { inputs: vec![], outputs: vec![ProcedureField {
            name: "x".into(), type_name: "ANY".into(), nullable: true,
        }] }, rows: vec![],
    });
    for query in ["CALL test.arg(1)", "CALL test.arg(rand())", "CALL test.output()"] {
        let plan = prepare(query, &BTreeMap::new(), Some(&catalog)).unwrap();
        assert!(!cypher_noop::is_noop(&plan, &catalog), "{query}");
    }
}

#[test]
fn unregistered_or_mutating_calls_are_never_certified() {
    use orchiddb::ir::plan::Node;
    let catalog = catalog();
    let mut plan = prepare("CALL test.noop", &BTreeMap::new(), Some(&catalog)).unwrap();
    assert!(!cypher_noop::is_noop(&plan, &ProcedureCatalog::new()));
    let Node::GraphReturn { input, .. } = plan.root.as_mut() else { panic!() };
    let Node::GraphProcedureCall { mode, .. } = input.as_mut() else { panic!() };
    *mode = ProcedureMode::Write;
    assert!(!cypher_noop::is_noop(&plan, &catalog));
}
