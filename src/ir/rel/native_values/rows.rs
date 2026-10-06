//! Arrow transport for compiled host programs. Physical scalar/element columns
//! keep using the existing SQL-island decoder; only opaque value cells differ.
use super::*;
use arrow::array::{Array, ArrayRef, RecordBatchOptions, StringArray, StructArray};
use arrow::buffer::NullBuffer;
use arrow::datatypes::SchemaRef;
use crate::ir::diagnostics::QueryExecutionError;
use crate::ir::policy::{GraphPlanPolicy, ResultForm};
use crate::ir::runtime::{ReturnedBatches, Row};

type QueryResult<T> = Result<T, QueryExecutionError>;

pub fn result_schema(fields: &[String]) -> SchemaRef {
    Arc::new(Schema::new(fields.iter().map(|name| Field::new(name, value_type(), true)).collect::<Vec<_>>()))
}

pub fn result_batch(
    fields: &[String], result_form: ResultForm, rows: &[Row], graph: &PropertyGraph,
    policy: &GraphPlanPolicy,
) -> QueryResult<RecordBatch> {
    let expand_bulk = policy.language == Language::Gremlin && result_form == ResultForm::TraverserStream;
    let count = |row: &Row| -> QueryResult<usize> {
        if expand_bulk {usize::try_from(row.bulk).map_err(|_|"Result multiplicity exceeds address space".into())} else {Ok(1)}
    };
    let row_count = rows.iter().try_fold(0usize, |sum, row| {
        sum.checked_add(count(row)?).ok_or_else(|| QueryExecutionError::from("Result multiplicity exceeds address space"))
    })?;
    let values = rows.iter().flat_map(|row| fields.iter().filter_map(|field|row.bindings.get(field))).collect::<Vec<_>>();
    if let Some(source) = &graph.source {
        source.prefetch(&mut values.iter().copied());
        source.check()?;
    }
    let mut context = Context::default();
    for value in values { context.capture(value, graph)?; }
    graph.check_source()?;
    let mut columns = Vec::with_capacity(fields.len());
    for field in fields {
        let mut encoded = Vec::with_capacity(row_count);
        for row in rows {
            let value = row.get(field);
            let wire = context.wire(&value);
            let text = wire.get(FIELD).and_then(serde_json::Value::as_str).map(str::to_owned);
            encoded.extend(std::iter::repeat_n(text, count(row)?));
        }
        let validity = NullBuffer::from(encoded.iter().map(Option::is_some).collect::<Vec<_>>());
        let strings: ArrayRef = Arc::new(StringArray::from(encoded));
        let DataType::Struct(carrier_fields) = value_type() else {unreachable!()};
        columns.push(Arc::new(StructArray::new(carrier_fields,vec![strings],Some(validity))) as ArrayRef);
    }
    RecordBatch::try_new_with_options(result_schema(fields), columns,
        &RecordBatchOptions::new().with_row_count(Some(row_count))).map_err(|e|e.to_string().into())
}

fn decode_value(value: Value, context: &mut Context) -> Result<Value, String> {
    match value {
        Value::Scalar(ScalarValue::Struct(array)) if is_value(array.data_type()) => {
            if array.is_null(0) {return Ok(Value::Null);}
            let data = array.column(0).as_any().downcast_ref::<StringArray>().ok_or("Invalid native carrier column")?;
            if data.is_null(0) {Ok(Value::Null)} else {context.decode(data.value(0))}
        }
        Value::List(values) => values.into_iter().map(|v|decode_value(v,context)).collect::<Result<Vec<_>,_>>().map(Value::List),
        Value::Map(values) => {
            if values.len() == 1 && let Some(Value::String(text)) = values.get(FIELD) {return context.decode(text);}
            values.into_iter().map(|(k,v)|Ok((k,decode_value(v,context)?))).collect::<Result<_,String>>().map(Value::Map)
        }
        value => Ok(value),
    }
}

pub fn decode_rows(batch: &RecordBatch, fields: &[String], graph: &PropertyGraph) -> QueryResult<Vec<Row>> {
    let returned = ReturnedBatches {fields:fields.to_vec(),result_form:ResultForm::RowSet,batch:batch.clone()};
    let (bindings, rows) = crate::ir::exec::batch_to_bindings(&returned).ok_or("Unsupported Arrow boundary value")?;
    let mut context = Context::default();
    let rows = rows.into_iter().map(|values| {
        let values = values.into_iter().map(|value|decode_value(value,&mut context)).collect::<Result<Vec<_>,_>>()?;
        Ok(Row {bindings:bindings.iter().cloned().zip(values).collect(),bulk:1})
    }).collect::<QueryResult<Vec<_>>>()?;
    // Returned graph references remain references to the caller's graph. Read
    // its authoritative metadata instead of installing a detached snapshot as
    // inserted graph entities (which would alter subsequent source scans).
    graph.prefetch_source(&rows);
    graph.check_source()?;
    Ok(rows)
}
