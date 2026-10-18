//! Lossless SQL/Arrow transport for precomputed Cypher temporal values.
//! Ordinary dates use native Date32 storage; other temporal values retain an
//! encoded payload. Semantic lowering still controls comparison/arithmetic.
use super::TemporalValue;
use crate::ir::functions::domain;
use arrow::datatypes::DataType;
use datafusion::common::{DataFusionError, Result, ScalarValue};

const NAME: &str = "cypher_temporal";

pub(crate) fn scalar(value: &TemporalValue) -> Result<ScalarValue> {
    // Keep ordinary dates as native SQL dates. Other temporal kinds and
    // dates outside the portable calendar range retain lossless text storage.
    if let TemporalValue::Date(date) = value {
        use chrono::Datelike;
        if (1..=9999).contains(&date.year()) {
            return domain::scalar(NAME, ScalarValue::Date32(Some(date.num_days_from_ce() - 719163)));
        }
    }
    encoded_scalar(value)
}

pub(crate) fn encoded_scalar(value: &TemporalValue) -> Result<ScalarValue> {
    domain::scalar(NAME, ScalarValue::Utf8(Some(value.encode())))
}

pub(crate) fn is_date_type(ty: &DataType) -> bool {
    *ty == DataType::Date32 || domain::descriptor(ty) == Some((NAME, &DataType::Date32))
}

pub(crate) fn encoded_kind(ty: &DataType) -> Option<&str> {
    let (name, storage) = domain::descriptor(ty)?;
    if storage != &DataType::Utf8 { return None; }
    let kind = name.strip_prefix("cypher_temporal_")?;
    if kind == "datetime_fixed" { return Some("datetime"); }
    matches!(kind, "date" | "localtime" | "time" | "localdatetime" | "datetime" | "duration").then_some(kind)
}

pub(crate) fn is_type(ty: &DataType) -> bool {
    is_date_type(ty) || domain::descriptor(ty) == Some((NAME, &DataType::Utf8)) || encoded_kind(ty).is_some()
}

pub(crate) fn scalar_for_type(value: &TemporalValue, ty: &DataType) -> Result<ScalarValue> {
    let result = if let Some(kind) = encoded_kind(ty) {
        if value.kind() != kind || (domain::descriptor(ty).unwrap().0 == "cypher_temporal_datetime_fixed" && !matches!(value, TemporalValue::DateTime(_, None))) {
            return Err(DataFusionError::Execution("Temporal kind does not fit its declared storage type".into()));
        }
        domain::scalar(domain::descriptor(ty).unwrap().0, ScalarValue::Utf8(Some(value.encode())))?
    } else if is_date_type(ty) {
        let days=value.calendar_date().ok_or_else(||DataFusionError::Execution("Expected date".into()))?.epoch_days();
        if value.kind()!="date"{return Err(DataFusionError::Execution("Expected date".into()));}
        let storage = ScalarValue::Date32(Some(i32::try_from(days).map_err(|_|DataFusionError::Execution("Date exceeds native DATE storage range".into()))?));
        if *ty == DataType::Date32 { storage } else { domain::scalar(NAME, storage)? }
    } else { encoded_scalar(value)? };
    if result.data_type() != *ty {
        return Err(DataFusionError::Execution("Temporal value does not fit its declared storage type".into()));
    }
    Ok(result)
}

pub(crate) fn contains_type(ty: &DataType) -> bool {
    is_type(ty)
        || match ty {
            DataType::List(f) | DataType::LargeList(f) | DataType::FixedSizeList(f, _) => {
                contains_type(f.data_type())
            }
            DataType::Struct(fields) => fields.iter().any(|f| contains_type(f.data_type())),
            _ => false,
        }
}

pub(crate) fn decode(value: &ScalarValue) -> Result<TemporalValue> {
    let storage = if value.data_type() == DataType::Date32 { value.clone() } else { domain::storage(value)? };
    match storage {
        ScalarValue::Date32(Some(days)) => {
            let date = super::CalendarDate::from_epoch_days(i64::from(days))
                .map_err(DataFusionError::Execution)?;
            Ok(match date.chrono() {
                Some(date) => TemporalValue::Date(date),
                None => TemporalValue::WideDate(date),
            })
        }
        ScalarValue::Utf8(Some(payload)) => {
            let decoded = TemporalValue::decode(&payload).map_err(DataFusionError::Execution)?;
            if encoded_kind(&value.data_type()).is_some_and(|kind| kind != decoded.kind())
                || (domain::descriptor(&value.data_type()).is_some_and(|(name,_)| name == "cypher_temporal_datetime_fixed") && !matches!(decoded, TemporalValue::DateTime(_, None))) {
                return Err(DataFusionError::Execution("Temporal payload violates its declared kind".into()));
            }
            Ok(decoded)
        }
        _ => Err(DataFusionError::Execution(
            "Invalid Cypher temporal transport payload".into(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{Value, policy::ResultForm, runtime::ReturnedBatches};
    use arrow::{
        array::RecordBatch,
        datatypes::{Field, Schema},
    };
    use std::sync::Arc;

    #[test]
    fn sql_boundary_restores_temporal_identity_and_nulls() {
        let expected = crate::ir::temporal::parse("time", "12:31:14.645876123+01:00").unwrap();
        let value = scalar(&expected).unwrap();
        let array = ScalarValue::iter_to_array(vec![
            value.clone(),
            ScalarValue::try_from(&value.data_type()).unwrap(),
        ])
        .unwrap();
        let batch = RecordBatch::try_new(
            Arc::new(Schema::new(vec![Field::new("t", value.data_type(), true)])),
            vec![array],
        )
        .unwrap();
        let (_, rows) = crate::ir::exec::batch_to_bindings(&ReturnedBatches {
            fields: vec!["t".into()],
            result_form: ResultForm::RowSet,
            batch,
        })
        .unwrap();
        assert!(matches!(&rows[0][0], Value::Temporal(actual) if actual == &expected));
        assert!(matches!(&rows[1][0], Value::Null));
        let list = ScalarValue::List(ScalarValue::new_list_nullable(
            &[value.clone()],
            &value.data_type(),
        ));
        let batch = RecordBatch::try_new(
            Arc::new(Schema::new(vec![Field::new("ts", list.data_type(), true)])),
            vec![list.to_array().unwrap()],
        )
        .unwrap();
        let (_, rows) = crate::ir::exec::batch_to_bindings(&ReturnedBatches {
            fields: vec!["ts".into()],
            result_form: ResultForm::RowSet,
            batch,
        })
        .unwrap();
        assert!(
            matches!(&rows[0][0], Value::List(items) if matches!(&items[0], Value::Temporal(actual) if actual == &expected))
        );
    }
}
