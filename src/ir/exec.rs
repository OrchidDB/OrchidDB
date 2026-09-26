//! Managed execution statistics and relational Arrow result decoding.
//!
//! Query execution is scheduled by the DataFusion relational DAG.
use crate::ir::catalog::array_value;
use crate::ir::plan::Node;
use crate::ir::runtime::ReturnedBatches;
use crate::ir::value::Value;
use arrow::array::Array;
use arrow::datatypes::{DataType, Field};

#[derive(Debug, Clone, Default)]
pub struct ExecStats {
    /// Operators scheduled by the DataFusion relational DAG.
    pub datafusion_ops: usize,
    /// Regions delegated to DuckDB.
    pub islands: usize,
    /// Rows produced by delegated regions when recorded by the engine.
    pub island_rows: usize,
}

impl ExecStats {
    /// Whether all query operators were delegated to SQL.
    pub fn fully_pushed_down(&self) -> bool {
        self.islands >= 1 && self.datafusion_ops == 0
    }
}

/// Column-name suffixes the relational lowering uses to encode a graph
/// binding across several flat columns. Kept in sync with `ir::rel`.
const ID_SUFFIX: &str = "__id";
const LABEL_SUFFIX: &str = "__label";
const PROP_MARKER: &str = "__prop__";
const SRC_ID_SUFFIX: &str = "__src_id";
const SRC_LABEL_SUFFIX: &str = "__src_label";
const DST_ID_SUFFIX: &str = "__dst_id";
const DST_LABEL_SUFFIX: &str = "__dst_label";
/// Separator between a `x.*` projection alias and each expanded property.
const STAR_SEP: &str = "__star__";

pub(crate) fn batch_to_bindings(
    returned: &ReturnedBatches,
) -> Option<(Vec<String>, Vec<Vec<Value>>)> {
    let batch = &returned.batch;
    let schema = batch.schema();
    let names: Vec<String> = schema
        .fields()
        .iter()
        .map(|field| field.name().to_string())
        .collect();

    enum Source {
        /// `(id column, label column)`
        NodeCols(usize, usize),
        /// `(src id, src label, dst id, dst label, edge id, edge label)`
        EdgeCols(usize, usize, usize, usize, Option<usize>, Option<usize>),
        /// A `x.*` projection: `(property name, column)` in projection order.
        StarCols(Vec<(String, usize)>),
        Scalar(usize),
    }

    let find = |suffix: &str, binding: &str| -> Option<usize> {
        names
            .iter()
            .position(|n| n == &format!("{binding}{suffix}"))
    };

    let mut bindings: Vec<String> = Vec::new();
    let mut sources: Vec<Source> = Vec::new();
    let mut seen: Vec<String> = Vec::new();

    // A `x.*` projection fans one field out into one column per property,
    // but the residual still refers to the single field `x.*`. Direct
    // evaluation represents that field as one binding holding a map of
    // property to value, which `finalize_return` expands into columns — so
    // collapse the columns back into exactly that shape.
    let mut star_groups: Vec<(String, Vec<(String, usize)>)> = Vec::new();
    for (index, name) in names.iter().enumerate() {
        let Some((field, key)) = name.split_once(STAR_SEP) else {
            continue;
        };
        match star_groups
            .iter_mut()
            .find(|(existing, _)| existing == field)
        {
            Some((_, columns)) => columns.push((key.to_string(), index)),
            None => star_groups.push((field.to_string(), vec![(key.to_string(), index)])),
        }
    }
    for (field, columns) in star_groups {
        bindings.push(field);
        sources.push(Source::StarCols(columns));
    }

    for (index, name) in names.iter().enumerate() {
        if name.contains(PROP_MARKER) || name.contains(STAR_SEP) {
            continue;
        }
        // Derive the binding name from whichever structural suffix matched.
        let binding = [
            SRC_ID_SUFFIX,
            SRC_LABEL_SUFFIX,
            DST_ID_SUFFIX,
            DST_LABEL_SUFFIX,
            ID_SUFFIX,
            LABEL_SUFFIX,
        ]
        .iter()
        .find_map(|suffix| name.strip_suffix(*suffix))
        .map(str::to_string);

        match binding {
            Some(binding) => {
                if seen.contains(&binding) {
                    continue;
                }
                seen.push(binding.clone());
                let src_id = find(SRC_ID_SUFFIX, &binding);
                let dst_id = find(DST_ID_SUFFIX, &binding);
                if let (Some(src_id), Some(dst_id)) = (src_id, dst_id) {
                    let src_label = find(SRC_LABEL_SUFFIX, &binding)?;
                    let dst_label = find(DST_LABEL_SUFFIX, &binding)?;
                    bindings.push(binding.clone());
                    sources.push(Source::EdgeCols(
                        src_id,
                        src_label,
                        dst_id,
                        dst_label,
                        find(ID_SUFFIX, &binding),
                        find(LABEL_SUFFIX, &binding),
                    ));
                } else {
                    let id = find(ID_SUFFIX, &binding)?;
                    let label = find(LABEL_SUFFIX, &binding)?;
                    bindings.push(binding.clone());
                    sources.push(Source::NodeCols(id, label));
                }
            }
            None => {
                bindings.push(name.clone());
                sources.push(Source::Scalar(index));
            }
        }
    }

    let column_value = |index: usize, row: usize| -> Option<Value> {
        decode_value(batch.column(index).as_ref(), row, Some(schema.field(index)))
    };
    let as_i64 = |value: &Value| -> Option<i64> {
        match value {
            Value::Int(v) | Value::Long(v) => Some(*v),
            _ => None,
        }
    };
    let as_label = |value: &Value| -> Option<String> {
        match value {
            Value::String(v) => Some(v.clone()),
            _ => None,
        }
    };

    let mut rows = Vec::with_capacity(batch.num_rows());
    for row in 0..batch.num_rows() {
        let mut out = Vec::with_capacity(sources.len());
        for source in &sources {
            let value = match source {
                Source::Scalar(index) => column_value(*index, row)?,
                Source::StarCols(columns) => {
                    let mut map = std::collections::BTreeMap::new();
                    // The map is sorted, so the projection order has to be
                    // recorded separately or the columns come out
                    // alphabetized instead of as written.
                    let mut order = Vec::with_capacity(columns.len());
                    for (key, index) in columns {
                        map.insert(key.clone(), column_value(*index, row)?);
                        order.push(Value::String(key.clone()));
                    }
                    map.insert(
                        crate::ir::value::STRUCT_ORDER_KEY.to_string(),
                        Value::List(order),
                    );
                    Value::Map(map)
                }
                Source::NodeCols(id, label) => {
                    let id_value = column_value(*id, row)?;
                    let label_value = column_value(*label, row)?;
                    match (as_i64(&id_value), as_label(&label_value)) {
                        (Some(id), Some(label)) => Value::Node { label, id },
                        // A null id is an outer-join miss, not a broken
                        // encoding; anything else means the island did not
                        // produce the shape we expect, so decline the island
                        // rather than fabricate a binding.
                        _ if matches!(id_value, Value::Null) => Value::Null,
                        _ => return None,
                    }
                }
                Source::EdgeCols(src_id, src_label, dst_id, dst_label, id, label) => {
                    let src_id_value = column_value(*src_id, row)?;
                    if matches!(src_id_value, Value::Null) {
                        Value::Null
                    } else {
                        Value::Edge {
                            rel_type: label
                                .and_then(|index| column_value(index, row))
                                .as_ref()
                                .and_then(as_label)
                                .unwrap_or_default(),
                            id: id
                                .and_then(|index| column_value(index, row))
                                .as_ref()
                                .and_then(as_i64)
                                .unwrap_or_default(),
                            src_label: as_label(&column_value(*src_label, row)?)?,
                            src_id: as_i64(&src_id_value)?,
                            dst_label: as_label(&column_value(*dst_label, row)?)?,
                            dst_id: as_i64(&column_value(*dst_id, row)?)?,
                            projected_properties: None,
                        }
                    }
                }
            };
            out.push(value);
        }
        rows.push(out);
    }

    Some((bindings, rows))
}

/// Decode one Arrow cell into a [`Value`], or `None` if the type has no
/// faithful representation here.
///
/// `None` is load-bearing: it makes the whole island decline, so the subtree
/// falls back to direct evaluation. The alternative — substituting `Null` for
/// anything unrecognized, which is what [`array_value`] does by design for
/// property reads — turns an unsupported type into a silently wrong answer.
/// That is exactly how `collect()` results were being dropped.
fn decode_value(array: &dyn Array, row: usize, field: Option<&Field>) -> Option<Value> {
    if row >= array.len() || array.is_null(row) {
        return Some(Value::Null);
    }
    match array.data_type() {
        DataType::Null => Some(Value::Null),
        DataType::Boolean | DataType::Int32 | DataType::Float64 | DataType::Utf8 => {
            Some(array_value(array, row, field))
        }
        // Preserve the declared Arrow widths at the language boundary.
        DataType::Int8 => array
            .as_any()
            .downcast_ref::<arrow::array::Int8Array>()
            .map(|typed| Value::Byte(typed.value(row))),
        DataType::Int16 => array
            .as_any()
            .downcast_ref::<arrow::array::Int16Array>()
            .map(|typed| Value::Short(typed.value(row))),
        DataType::Int64 => array
            .as_any()
            .downcast_ref::<arrow::array::Int64Array>()
            .map(|typed| Value::Long(typed.value(row))),
        DataType::UInt8 => array
            .as_any()
            .downcast_ref::<arrow::array::UInt8Array>()
            .map(|typed| Value::UInt8(typed.value(row))),
        DataType::UInt16 => array
            .as_any()
            .downcast_ref::<arrow::array::UInt16Array>()
            .map(|typed| Value::UInt16(typed.value(row))),
        DataType::UInt32 => array
            .as_any()
            .downcast_ref::<arrow::array::UInt32Array>()
            .map(|typed| Value::UInt32(typed.value(row))),
        DataType::UInt64 => array
            .as_any()
            .downcast_ref::<arrow::array::UInt64Array>()
            .map(|typed| Value::UInt64(typed.value(row))),
        // The lowering uses zero-scale decimals for 128-bit integers and
        // scaled decimals for `DECIMAL(p, s)`; the value representation holds those as
        // `BigInt` and a `BigDecimal` carrying the declared scale.
        DataType::Decimal128(_, scale) => {
            let typed = array
                .as_any()
                .downcast_ref::<arrow::array::Decimal128Array>()?;
            let unscaled = num_bigint::BigInt::from(typed.value(row));
            if *scale == 0 {
                Some(Value::BigInt(unscaled))
            } else if *scale > 0 {
                Some(Value::BigDecimal(bigdecimal::BigDecimal::new(
                    unscaled,
                    i64::from(*scale),
                )))
            } else {
                None
            }
        }
        DataType::Float32 => array
            .as_any()
            .downcast_ref::<arrow::array::Float32Array>()
            .map(|typed| Value::Float32(typed.value(row))),
        DataType::LargeUtf8 => array
            .as_any()
            .downcast_ref::<arrow::array::LargeStringArray>()
            .map(|typed| Value::String(typed.value(row).to_string())),
        DataType::Utf8View => array
            .as_any()
            .downcast_ref::<arrow::array::StringViewArray>()
            .map(|typed| Value::String(typed.value(row).to_string())),
        // `collect()` and friends produce real Arrow lists; decode them
        // elementwise so nested lists work too.
        DataType::List(inner) => {
            let typed = array.as_any().downcast_ref::<arrow::array::ListArray>()?;
            decode_list(typed.value(row).as_ref(), inner)
        }
        DataType::LargeList(inner) => {
            let typed = array
                .as_any()
                .downcast_ref::<arrow::array::LargeListArray>()?;
            decode_list(typed.value(row).as_ref(), inner)
        }
        _ => None,
    }
}

fn decode_list(items: &dyn Array, inner: &Field) -> Option<Value> {
    let mut out = Vec::with_capacity(items.len());
    for index in 0..items.len() {
        out.push(decode_value(items, index, Some(inner))?);
    }
    Some(Value::List(out))
}

/// Does this subtree write to the graph?
pub fn contains_mutation(node: &Node) -> bool {
    crate::ir::analysis::contains_source_mutation(node)
}

impl From<crate::ir::rel::dag::DagStats> for ExecStats {
    fn from(stats: crate::ir::rel::dag::DagStats) -> Self {
        Self {
            islands: stats.duckdb_regions,
            datafusion_ops: stats.datafusion_operators,
            ..Default::default()
        }
    }
}
