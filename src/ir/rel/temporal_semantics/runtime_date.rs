//! Runtime constructors whose input domain proves identical native/SQL behavior.
//! Domain inspection checks grammar/range only; it never computes date outputs.
use super::*;
use crate::ir::functions::{domain, logical::{LogicalFunction, SqlFunctionMapping}};
use datafusion::logical_expr::{ColumnarValue, Volatility, create_udf};

fn canonical_date(text: &str) -> bool {
    let b = text.as_bytes();
    if b.len() != 10 || b[4] != b'-' || b[7] != b'-'
        || b.iter().enumerate().any(|(i,c)| i != 4 && i != 7 && !c.is_ascii_digit()) {
        return false;
    }
    let year: u32 = text[..4].parse().unwrap();
    let month: u32 = text[5..7].parse().unwrap();
    let day: u32 = text[8..].parse().unwrap();
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let days = match month { 1|3|5|7|8|10|12 => 31, 4|6|9|11 => 30, 2 => if leap {29} else {28}, _ => 0 };
    year != 0 && day != 0 && day <= days
}

fn safe_value(value: &ScalarValue) -> bool {
    if value.is_null() { return true; }
    match value {
        ScalarValue::Utf8(Some(text)) | ScalarValue::LargeUtf8(Some(text))
        | ScalarValue::Utf8View(Some(text)) => canonical_date(text),
        _ => false,
    }
}

fn native_date(value: ScalarValue) -> datafusion::common::Result<ScalarValue> {
    let ty = domain::data_type("cypher_temporal", DataType::Date32);
    if value.is_null() { return ScalarValue::try_from(&ty); }
    let text = match value {
        ScalarValue::Utf8(Some(text)) | ScalarValue::LargeUtf8(Some(text))
        | ScalarValue::Utf8View(Some(text)) => text,
        _ => return Err(DataFusionError::Execution("date requires a string".into())),
    };
    let value = crate::ir::temporal::construct("date", &crate::ir::Value::String(text))
        .map_err(DataFusionError::Execution)?;
    let crate::ir::Value::Temporal(date) = value else {
        return Err(DataFusionError::Execution("date constructor returned a non-date".into()));
    };
    crate::ir::temporal::transport::scalar_for_type(&date, &ty)
}

pub(in crate::ir::rel) fn lower_runtime_date(
    context: &LoweringContext<'_>, plan: &LogicalPlan, expr: &IrExpr,
) -> Option<Expr> {
    let IrExpr::Call { name, args } = expr else { return None; };
    if name != "cypher_temporal.date" || args.len() != 1 { return None; }
    // Do not recursively enter the temporal fence for a typed identity.
    if let IrExpr::Binding(binding) = &args[0]
        && binding_type(plan, binding).is_some_and(crate::ir::temporal::transport::is_date_type) {
        return Some(col_exact(&resolve_column_name(plan, binding)?));
    }
    let operand = context.lower_expr(plan, &args[0]).ok()?;
    let values = match &operand {
        Expr::Literal(value, _) => vec![value.clone()],
        Expr::Column(column) => finite_values(plan, &column.name)?,
        _ => return None,
    };
    let input_type = operand.get_type(plan.schema()).ok()?;
    if !matches!(input_type, DataType::Utf8 | DataType::LargeUtf8 | DataType::Utf8View | DataType::Null)
        || !values.iter().all(safe_value) { return None; }
    let native = create_udf("cypher_runtime_canonical_date", vec![input_type],
        domain::data_type("cypher_temporal", DataType::Date32), Volatility::Immutable,
        Arc::new(|args| {
            match &args[0] {
                ColumnarValue::Scalar(value) => native_date(value.clone()).map(ColumnarValue::Scalar),
                ColumnarValue::Array(array) => {
                    let values = (0..array.len()).map(|i| native_date(ScalarValue::try_from_array(array,i)?))
                        .collect::<datafusion::common::Result<Vec<_>>>()?;
                    if values.is_empty() {
                        return Ok(ColumnarValue::Array(arrow::array::new_empty_array(
                            &domain::data_type("cypher_temporal", DataType::Date32))));
                    }
                    ScalarValue::iter_to_array(values).map(ColumnarValue::Array)
                }
            }
        }));
    Some(LogicalFunction::new("cypher_runtime_canonical_date", Arc::new(native),
        BTreeMap::from([("duckdb".into(), SqlFunctionMapping {
            value: "CAST(__arg0 AS DATE)".into(), ordering: None,
        })])).into_udf().call(vec![operand]))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn canonical_grammar_calendar_and_range_are_proven_without_constructing_dates() {
        for text in ["0001-01-01", "9999-12-31", "2000-02-29", "2024-02-29"] { assert!(canonical_date(text), "{text}"); }
        for text in ["0000-01-01", "1900-02-29", "2023-02-29", "2024-04-31", "2024-00-01", "2024-01-00", "2024-1-01", "20240101", "2024-W01-1", "2024-001", "+10000-01-01", "é024-01-01"] { assert!(!canonical_date(text), "{text}"); }
    }
}
