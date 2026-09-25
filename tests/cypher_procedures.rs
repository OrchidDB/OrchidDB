#![cfg(feature = "duckdb")]
use new_graph::{
    engine::GraphEngine,
    ir::{
        Value,
        procedures::{ProcedureField, ProcedureSignature, TableProcedure},
    },
};
use std::collections::BTreeMap;

fn field(name: &str, kind: &str) -> ProcedureField {
    ProcedureField {
        name: name.into(),
        type_name: kind.into(),
        nullable: true,
    }
}

#[tokio::test]
async fn registered_procedures_execute_through_normal_engine() {
    let mut engine = GraphEngine::in_memory().unwrap();
    engine
        .register_table_procedure(
            "example.lookup".into(),
            TableProcedure {
                signature: ProcedureSignature {
                    inputs: vec![field("id", "INTEGER")],
                    outputs: vec![field("name", "STRING")],
                },
                rows: vec![
                    vec![Value::Int(1), Value::String("one".into())],
                    vec![Value::Int(2), Value::String("two".into())],
                ],
            },
        )
        .unwrap();
    let result = engine
        .cypher("UNWIND [1,2] AS id CALL example.lookup(id) YIELD name RETURN name ORDER BY name")
        .await
        .unwrap();
    assert_eq!(result.returned.batch.num_rows(), 2);
    let result = engine
        .cypher_with_params(
            "CALL example.lookup",
            &BTreeMap::from([("id".into(), Value::Int(1))]),
        )
        .await
        .unwrap();
    assert_eq!(result.returned.batch.num_rows(), 1);
    assert_eq!(result.returned.batch.schema().field(0).name(), "name");
    assert!(engine.cypher("CALL example.lookup(true)").await.is_err());
    assert!(engine.cypher("CALL example.missing()").await.is_err());
    assert_eq!(
        engine
            .cypher("CALL example.lookup(9)")
            .await
            .unwrap()
            .returned
            .batch
            .num_rows(),
        0
    );
}

#[tokio::test]
async fn void_procedure_preserves_input_rows_without_return_columns() {
    let mut engine = GraphEngine::in_memory().unwrap();
    engine
        .register_table_procedure(
            "example.noop".into(),
            TableProcedure {
                signature: ProcedureSignature {
                    inputs: vec![],
                    outputs: vec![],
                },
                rows: vec![],
            },
        )
        .unwrap();
    let result = engine
        .cypher("UNWIND [1,2,3] AS x CALL example.noop() RETURN x")
        .await
        .unwrap();
    assert_eq!(result.returned.batch.num_rows(), 3);
    let result = engine.cypher("CALL example.noop()").await.unwrap();
    assert_eq!(result.returned.batch.num_rows(), 0);
    assert_eq!(result.returned.batch.num_columns(), 0);
}
