//! JSON aggregates retain SQL NULL inputs as JSON null and input order.
use super::*;
use datafusion::logical_expr::{
    Accumulator, AggregateUDF, AggregateUDFImpl,
    function::{AccumulatorArgs, StateFieldsArgs},
};
#[derive(Debug, PartialEq, Eq, Hash)]
struct JsonAggregate {
    name: String,
    aliases: Vec<String>,
    object: bool,
    signature: Signature,
}
pub fn aggregate_names() -> impl Iterator<Item = &'static str> {
    ["json.array_agg", "json.object_agg"].into_iter()
}
pub fn aggregate(name: &str) -> Option<Arc<AggregateUDF>> {
    static FUNCTIONS: LazyLock<BTreeMap<String, Arc<AggregateUDF>>> = LazyLock::new(|| {
        [("array_agg", false), ("object_agg", true)]
            .into_iter()
            .map(|(name, object)| {
                (
                    format!("json.{name}"),
                    Arc::new(AggregateUDF::new_from_impl(JsonAggregate {
                        name: format!("__orchiddb_json_{name}"),
                        aliases: vec![format!("json.{name}")],
                        object,
                        signature: Signature::any(
                            if object { 2 } else { 1 },
                            Volatility::Immutable,
                        ),
                    })),
                )
            })
            .collect()
    });
    let normalized = name.to_ascii_lowercase();
    let name = normalized
        .strip_prefix("__orchiddb_json_")
        .map(|s| format!("json.{s}"))
        .unwrap_or(normalized);
    FUNCTIONS.get(&name).cloned()
}
impl AggregateUDFImpl for JsonAggregate {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn name(&self) -> &str {
        &self.name
    }
    fn aliases(&self) -> &[String] {
        &self.aliases
    }
    fn signature(&self) -> &Signature {
        &self.signature
    }
    fn return_type(&self, args: &[DataType]) -> Result<DataType> {
        if self.object
            && !matches!(
                args.first(),
                Some(DataType::Utf8 | DataType::LargeUtf8 | DataType::Utf8View | DataType::Null)
            )
        {
            return Err(error("json.object_agg keys must be text"));
        }
        Ok(domain::json_type())
    }
    fn accumulator(&self, args: AccumulatorArgs) -> Result<Box<dyn Accumulator>> {
        Ok(Box::new(JsonAccumulator {
            object: self.object,
            values: vec![],
            ordering: vec![],
            sort_options: args.order_bys.iter().map(|s| s.options).collect(),
            order_types: args
                .order_bys
                .iter()
                .map(|s| s.expr.data_type(args.schema))
                .collect::<Result<Vec<_>>>()?,
            distinct: args.is_distinct,
            distinct_keys: vec![],
        }))
    }
    fn state_fields(&self, args: StateFieldsArgs) -> Result<Vec<FieldRef>> {
        Ok(vec![Arc::new(Field::new(
            format!("{}[json_state]", args.name),
            DataType::Utf8,
            false,
        ))])
    }
}
#[derive(Debug, Default)]
pub(super) struct JsonAccumulator {
    pub object: bool,
    pub values: Vec<Value>,
    pub ordering: Vec<Vec<ScalarValue>>,
    pub sort_options: Vec<arrow::compute::SortOptions>,
    pub order_types: Vec<DataType>,
    pub distinct: bool,
    pub distinct_keys: Vec<String>,
}
impl Accumulator for JsonAccumulator {
    fn update_batch(&mut self, values: &[ArrayRef]) -> Result<()> {
        for row in 0..values[0].len() {
            if self.distinct {
                let keys = values[..(1 + usize::from(self.object))]
                    .iter()
                    .map(|a| {
                        ScalarValue::try_from_array(a, row)
                            .map(|v| crate::ir::identity::encode_scalar(&v))
                    })
                    .collect::<Result<Vec<_>>>()?;
                self.distinct_keys
                    .push(serde_json::to_string(&keys).map_err(error)?);
            }
            self.ordering.push(
                values[(1 + usize::from(self.object))..]
                    .iter()
                    .map(|a| ScalarValue::try_from_array(a, row))
                    .collect::<Result<_>>()?,
            );
            let value = to_json(&ScalarValue::try_from_array(
                values[usize::from(self.object)].as_ref(),
                row,
            )?)?;
            if self.object {
                let key = text(&ScalarValue::try_from_array(values[0].as_ref(), row)?)?
                    .ok_or_else(|| error("json.object_agg key cannot be SQL NULL"))?;
                self.values
                    .push(Value::Array(vec![Value::String(key), value]));
            } else {
                self.values.push(value);
            }
        }
        Ok(())
    }
    fn evaluate(&mut self) -> Result<ScalarValue> {
        if self.values.is_empty() {
            return null(&domain::json_type());
        }
        let mut indices = (0..self.values.len()).collect::<Vec<_>>();
        if !self.sort_options.is_empty() {
            let columns = self
                .sort_options
                .iter()
                .enumerate()
                .map(|(i, options)| {
                    Ok(arrow::compute::SortColumn {
                        values: ScalarValue::iter_to_array(
                            self.ordering.iter().map(|row| row[i].clone()),
                        )?,
                        options: Some(*options),
                    })
                })
                .collect::<Result<Vec<_>>>()?;
            indices = arrow::compute::lexsort_to_indices(&columns, None)?
                .values()
                .iter()
                .map(|i| *i as usize)
                .collect();
        }
        if self.distinct {
            let mut seen = std::collections::BTreeSet::new();
            indices.retain(|i| seen.insert(self.distinct_keys[*i].clone()));
        }
        if self.object {
            let mut object = Map::new();
            for i in indices {
                let pair = self.values[i].as_array().unwrap();
                object.insert(pair[0].as_str().unwrap().into(), pair[1].clone());
            }
            output(&Value::Object(object))
        } else {
            output(&Value::Array(
                indices
                    .into_iter()
                    .map(|i| self.values[i].clone())
                    .collect(),
            ))
        }
    }
    fn state(&mut self) -> Result<Vec<ScalarValue>> {
        Ok(vec![ScalarValue::Utf8(Some(
            Value::Array(if self.sort_options.is_empty() && !self.distinct {
                self.values.clone()
            } else {
                self.values
                    .iter()
                    .zip(&self.ordering)
                    .enumerate()
                    .map(|(i, (value, keys))| {
                        Ok(Value::Array(vec![
                            value.clone(),
                            Value::Array(
                                keys.iter()
                                    .map(crate::federation::scalar_json)
                                    .collect::<Result<Vec<_>>>()?,
                            ),
                            self.distinct_keys
                                .get(i)
                                .cloned()
                                .map(Value::String)
                                .unwrap_or(Value::Null),
                        ]))
                    })
                    .collect::<Result<Vec<_>>>()?
            })
            .to_string(),
        ))])
    }
    fn merge_batch(&mut self, states: &[ArrayRef]) -> Result<()> {
        for row in 0..states[0].len() {
            let Some(state) = text(&ScalarValue::try_from_array(states[0].as_ref(), row)?)? else {
                continue;
            };
            let Value::Array(mut values) = serde_json::from_str(&state).map_err(error)? else {
                return Err(error("invalid JSON aggregate state"));
            };
            if self.sort_options.is_empty() && !self.distinct {
                self.values.append(&mut values);
            } else {
                for value in values {
                    let pair = value
                        .as_array()
                        .ok_or_else(|| error("invalid ordered aggregate state"))?;
                    self.values.push(pair[0].clone());
                    if self.distinct {
                        self.distinct_keys.push(
                            pair[2]
                                .as_str()
                                .ok_or_else(|| error("invalid distinct state"))?
                                .into(),
                        );
                    }
                    self.ordering.push(
                        pair[1]
                            .as_array()
                            .ok_or_else(|| error("invalid ordering state"))?
                            .iter()
                            .zip(&self.order_types)
                            .map(|(value, ty)| {
                                crate::federation::json_scalar(value, ty).map_err(error)
                            })
                            .collect::<Result<Vec<_>>>()?,
                    );
                }
            }
        }
        Ok(())
    }
    fn size(&self) -> usize {
        std::mem::size_of::<Self>()
            + self.distinct_keys.iter().map(String::len).sum::<usize>()
            + self
                .ordering
                .iter()
                .flatten()
                .map(ScalarValue::size)
                .sum::<usize>()
            + self
                .values
                .iter()
                .map(|v| v.to_string().len())
                .sum::<usize>()
    }
}
