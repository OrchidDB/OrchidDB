//! Embedded DuckDB bindings for the same scalar/value helpers as the extension.
//! SQL still owns scans, joins, filtering, grouping and ordering.
use crate::ir::rel::native_values as native;
use arrow::array::{Array, ArrayRef, StringArray};
use datafusion::common::ScalarValue;
use duckdb::{
    Connection,
    core::DataChunkHandle,
    core::LogicalTypeId,
    vscalar::{ScalarFunctionSignature, VScalar},
    vtab::arrow::{WritableVector, write_arrow_array_to_vector},
};
use serde_json::{Value, json};
use std::sync::Arc;

fn wire(value: ScalarValue) -> Result<Value, Box<dyn std::error::Error>> {
    if value.is_null() {
        return Ok(Value::Null);
    }
    Ok(match value {
        ScalarValue::Struct(a) => a
            .fields()
            .iter()
            .zip(a.columns())
            .map(|(f, a)| Ok((f.name().clone(), wire(ScalarValue::try_from_array(a, 0)?)?)))
            .collect::<Result<serde_json::Map<_, _>, Box<dyn std::error::Error>>>()?
            .into(),
        ScalarValue::List(a) => list(a.value(0))?,
        ScalarValue::LargeList(a) => list(a.value(0))?,
        ScalarValue::FixedSizeList(a) => list(a.value(0))?,
        ScalarValue::Utf8(Some(v))
        | ScalarValue::LargeUtf8(Some(v))
        | ScalarValue::Utf8View(Some(v)) => v.into(),
        ScalarValue::Boolean(Some(v)) => v.into(),
        ScalarValue::Int8(Some(v)) => json!({"__orchiddb_number":["TINYINT",v.to_string()]}),
        ScalarValue::Int16(Some(v)) => json!({"__orchiddb_number":["SMALLINT",v.to_string()]}),
        ScalarValue::Int32(Some(v)) => json!({"__orchiddb_number":["INTEGER",v.to_string()]}),
        ScalarValue::Int64(Some(v)) => json!({"__orchiddb_number":["BIGINT",v.to_string()]}),
        ScalarValue::Float32(Some(v)) => json!({"__orchiddb_number":["FLOAT",v.to_string()]}),
        ScalarValue::Float64(Some(v)) => json!({"__orchiddb_number":["DOUBLE",v.to_string()]}),
        other => {
            use base64::Engine;
            let value = crate::ir::value::scalar_semantic_value(&other)
                .ok_or_else(|| format!("unsupported host helper input {}", other.data_type()))?;
            json!({native::FIELD:base64::engine::general_purpose::STANDARD.encode(crate::ir::catalog::snapshot::binary::encode_value_bytes(&value))})
        }
    })
}
fn list(array: ArrayRef) -> Result<Value, Box<dyn std::error::Error>> {
    (0..array.len())
        .map(|i| wire(ScalarValue::try_from_array(&array, i)?))
        .collect::<Result<Vec<_>, _>>()
        .map(Value::Array)
}
struct Native;
impl VScalar for Native {
    type State = ();
    fn signatures() -> Vec<ScalarFunctionSignature> {
        vec![ScalarFunctionSignature::exact(
            vec![
                LogicalTypeId::Varchar.into(),
                LogicalTypeId::Varchar.into(),
                LogicalTypeId::Any.into(),
                LogicalTypeId::Bigint.into(),
                LogicalTypeId::Bigint.into(),
            ],
            LogicalTypeId::Varchar.into(),
        )]
    }
    unsafe fn invoke(
        _: &(),
        input: &mut DataChunkHandle,
        output: &mut dyn WritableVector,
    ) -> Result<(), Box<dyn std::error::Error>> {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let arrays = super::language_vectors::arrays(input)?;
            let mut out = Vec::new();
            for row in 0..input.len() {
                let text = |i: usize| -> Result<String, Box<dyn std::error::Error>> {
                    match ScalarValue::try_from_array(arrays[i].as_ref(), row)? {
                        ScalarValue::Utf8(Some(v)) => Ok(v),
                        _ => Err("expected helper string".into()),
                    }
                };
                let kind = text(0)?;
                let spec = text(1)?;
                let value =
                    wire(ScalarValue::try_from_array(&arrays[2], row)?)?["argument"].clone();
                let clock = |i: usize| -> Result<i64, Box<dyn std::error::Error>> {
                    match ScalarValue::try_from_array(arrays[i].as_ref(), row)? {
                        ScalarValue::Int64(Some(v)) => Ok(v),
                        _ => Err("expected helper clock".into()),
                    }
                };
                let result = match kind.as_str() {
                    "value" | "predicate" => {
                        native::evaluate(&spec, &value, kind == "predicate", clock(3)?, clock(4)?)
                            .map_err(|e| e.sql_error())?
                    }
                    "key" => json!(native::key(&value, spec == "true")?),
                    "items" => native::items(&value)?,
                    "aggregate" => native::aggregate(&spec, &value)?,
                    "project" => native::project(&spec, &value, clock(3)?, clock(4)?)
                        .map_err(|e| e.sql_error())?,
                    "sort" => json!(native::sort(&spec, &value)?),
                    _ => return Err("unknown native scalar operation".into()),
                };
                out.push(result.to_string());
            }
            let array: ArrayRef = Arc::new(StringArray::from(out));
            write_arrow_array_to_vector(&array, output)
        }))
        .unwrap_or_else(|_| Err("native language helper panicked".into()))
    }
}
pub(super) fn register(connection: &Connection) -> duckdb::Result<()> {
    if connection
        .prepare("SELECT __orchiddb_value('null', NULL) WHERE false")
        .is_ok()
    {
        return Ok(());
    }
    connection.register_scalar_function::<Native>("__orchiddb_native_eval")?;
    let call = |kind: &str, spec: &str, value: &str| {
        format!(
            "__orchiddb_native_eval('{kind}', {spec}, struct_pack(argument := {value}), epoch_us(current_timestamp), epoch_us(current_timestamp))"
        )
    };
    let shape = r#"'{"__orchiddb_value_v1":"VARCHAR"}'"#;
    let list_shape = r#"'[{"__orchiddb_value_v1":"VARCHAR"}]'"#;
    for (name, args, body) in [
        (
            native::VALUE,
            "spec, bindings",
            format!("from_json({}, {shape})", call("value", "spec", "bindings")),
        ),
        (
            native::PREDICATE,
            "spec, bindings",
            format!(
                r#"from_json({}, '"BOOLEAN"')"#,
                call("predicate", "spec", "bindings")
            ),
        ),
        (
            native::KEY,
            "value, cypher",
            format!(
                "json_extract_string({}, '$')",
                call("key", "CAST(cypher AS VARCHAR)", "value")
            ),
        ),
        (
            native::ITEMS,
            "value",
            format!("from_json({}, {list_shape})", call("items", "''", "value")),
        ),
        (
            native::AGGREGATE,
            "spec, input_values",
            format!(
                "from_json({}, {shape})",
                call("aggregate", "spec", "input_values")
            ),
        ),
        (
            native::PROJECT,
            "spec, bindings",
            format!(
                "from_json({}, {list_shape})",
                call("project", "spec", "bindings")
            ),
        ),
        (
            native::SORT,
            "spec, rows",
            format!(
                "list_transform(from_json({}, '[\"UBIGINT\"]'), i -> rows[CAST(i AS BIGINT) + 1])",
                call("sort", "spec", "rows")
            ),
        ),
    ] {
        connection.execute_batch(&format!("CREATE OR REPLACE TEMP MACRO {name}({args}) AS {body}"))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nullable_predicates_and_date_inputs_keep_sql_types() {
        let connection = Connection::open_in_memory().unwrap();
        register(&connection).unwrap();
        let mut statement = connection
            .prepare(
                r#"SELECT __orchiddb_predicate('{"Binding":"value"}', struct_pack(value := flag))
            FROM (VALUES (NULL::BOOLEAN), (false), (true)) AS input(flag)"#,
            )
            .unwrap();
        let actual = statement
            .query_map([], |row| row.get::<_, Option<bool>>(0))
            .unwrap()
            .collect::<duckdb::Result<Vec<_>>>()
            .unwrap();
        assert_eq!(actual, vec![None, Some(false), Some(true)]);
        let encoded: String = connection
            .query_row(
                r#"SELECT (__orchiddb_value('{"Binding":"value"}',
            struct_pack(value := DATE '2024-02-29'))).__orchiddb_value_v1"#,
                [],
                |row| row.get(0),
            )
            .unwrap();
        use base64::Engine;
        let value = crate::ir::catalog::snapshot::binary::decode_value_bytes(
            &base64::engine::general_purpose::STANDARD
                .decode(encoded)
                .unwrap(),
        )
        .unwrap();
        assert_eq!(
            value,
            crate::ir::value::Value::Temporal(
                crate::ir::temporal::parse("date", "2024-02-29").unwrap()
            )
        );
    }
}

#[cfg(test)]
mod diagnostic_tests {
    use super::*;
    #[test]
    fn runtime_diagnosis_survives_duckdb_scalar_error_transport() {
        let connection = Connection::open_in_memory().unwrap();
        register(&connection).unwrap();
        let spec =
            json!({"Call":{"name":"cypher_slice_bound","args":[{"Binding":"value"}]}}).to_string();
        let error = connection
            .query_row::<String, _, _>(
                "SELECT __orchiddb_value(?, struct_pack(value := -1))",
                [spec],
                |row| row.get(0),
            )
            .unwrap_err();
        let error = crate::ir::diagnostics::QueryExecutionError::from_error(error);
        assert_eq!(
            error.diagnosis,
            Some(crate::ir::diagnostics::RuntimeDiagnosis::NegativeIntegerArgument)
        );
        assert!(error.message.contains("non-negative"));
    }
}
