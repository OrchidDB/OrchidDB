//! Relational list contracts for Cypher collections.
//!
//! Structured catalog properties (`INT64[]`, `STRING[]`, nested lists) are
//! stored as display text so element rendering keeps the Kuzu format. List
//! operators need real list values instead: quantifiers, UNWIND, slicing,
//! membership and element access must never parse display text. Each scan
//! therefore carries a hidden, natively typed shadow column for every
//! list-valued property whose element type is uniform across the scan. List
//! consumers read the shadow; everything else keeps reading the display
//! column. A property whose values cannot be typed as one Arrow list type has
//! no shadow, and list consumers keep returning typed unsupported errors.

use arrow::array::new_null_array;

use super::*;

const LIST_SHADOW_SUFFIX: &str = "__w_list";

/// Hidden column holding the natively typed list value of `binding.property`.
pub(super) fn list_shadow_col(binding: &str, property: &str) -> String {
    format!("{}{LIST_SHADOW_SUFFIX}", prop_col(binding, property))
}

/// The native list column for a property reference, when the scan provided
/// one.
pub(super) fn native_list_property(plan: &LogicalPlan, expr: &IrExpr) -> Option<Expr> {
    let IrExpr::Property { binding, name, .. } = expr else {
        return None;
    };
    let shadow = list_shadow_col(binding, name);
    has_exact_col(plan, &shadow).then(|| col_exact(shadow))
}

/// Append list shadow columns to freshly built scan batches. `is_edge`
/// selects how element values are read back from the catalog; the reads are
/// overlay-aware, so mutated properties are typed from their current values.
pub(super) fn with_list_shadows(
    graph: &PropertyGraph,
    language: Language,
    binding: &str,
    candidates: &[String],
    is_edge: bool,
    batches: Vec<RecordBatch>,
) -> RelResult<Vec<RecordBatch>> {
    if language != Language::Cypher || candidates.is_empty() || batches.is_empty() {
        return Ok(batches);
    }
    let id_name = id_col(binding);
    let label_name = label_col(binding);
    // Values per candidate property, per batch.
    let mut values: Vec<Vec<Vec<Value>>> =
        vec![Vec::with_capacity(batches.len()); candidates.len()];
    for batch in &batches {
        let (Some(ids), Some(labels)) = (
            batch
                .column_by_name(&id_name)
                .and_then(|array| array.as_any().downcast_ref::<Int64Array>()),
            batch
                .column_by_name(&label_name)
                .and_then(|array| array.as_any().downcast_ref::<StringArray>()),
        ) else {
            return Ok(batches);
        };
        for (index, property) in candidates.iter().enumerate() {
            let mut column = Vec::with_capacity(batch.num_rows());
            for row in 0..batch.num_rows() {
                let (label, id) = (labels.value(row), ids.value(row));
                column.push(if is_edge {
                    graph.edge_property(label, id, property)
                } else {
                    graph.node_property(label, id, property)
                });
            }
            values[index].push(column);
        }
    }

    let mut shadows = Vec::new();
    for (index, property) in candidates.iter().enumerate() {
        let Some(data_type) = uniform_list_type(values[index].iter().flatten()) else {
            continue;
        };
        shadows.push((property, index, data_type));
    }
    if shadows.is_empty() {
        return Ok(batches);
    }

    let mut out = Vec::with_capacity(batches.len());
    for (batch_index, batch) in batches.into_iter().enumerate() {
        let mut fields = batch.schema().fields().iter().cloned().collect::<Vec<_>>();
        let mut columns = batch.columns().to_vec();
        for (property, index, data_type) in &shadows {
            fields.push(Arc::new(Field::new(
                list_shadow_col(binding, property),
                data_type.clone(),
                true,
            )));
            columns.push(list_array(&values[*index][batch_index], data_type)?);
        }
        out.push(RecordBatch::try_new(
            Arc::new(Schema::new(fields)),
            columns,
        )?);
    }
    Ok(out)
}

/// The one Arrow list type able to hold every non-null value exactly, if
/// there is one. At least one element must fix the element type.
pub(super) fn uniform_list_type<'a>(values: impl Iterator<Item = &'a Value>) -> Option<DataType> {
    let mut element: Option<DataType> = None;
    let mut saw_list = false;
    for value in values {
        match value {
            Value::Null => {}
            Value::List(items) => {
                saw_list = true;
                for item in items {
                    if matches!(item, Value::Null) {
                        continue;
                    }
                    let item_type = value_arrow_type(item)?;
                    match &element {
                        None => element = Some(item_type),
                        Some(existing) if *existing == item_type => {}
                        Some(existing) => element = Some(merge_list_types(existing, &item_type)?),
                    }
                }
            }
            _ => return None,
        }
    }
    if !saw_list {
        return None;
    }
    element.map(|element| DataType::List(Arc::new(Field::new("item", element, true))))
}

fn merge_list_types(left: &DataType, right: &DataType) -> Option<DataType> {
    // Nested lists may disagree only through an all-null inner list.
    match (left, right) {
        (DataType::List(left), DataType::List(right)) => {
            Some(DataType::List(Arc::new(Field::new(
                "item",
                merge_list_types(left.data_type(), right.data_type())?,
                true,
            ))))
        }
        (DataType::Null, other) | (other, DataType::Null) => Some(other.clone()),
        (left, right) if left == right => Some(left.clone()),
        _ => None,
    }
}

fn value_arrow_type(value: &Value) -> Option<DataType> {
    Some(match value {
        Value::Bool(_) => DataType::Boolean,
        Value::Int(_) | Value::Long(_) => DataType::Int64,
        Value::Float(_) => DataType::Float64,
        Value::String(_) => DataType::Utf8,
        Value::List(items) => {
            let mut element = DataType::Null;
            for item in items {
                if matches!(item, Value::Null) {
                    continue;
                }
                element = merge_list_types(&element, &value_arrow_type(item)?)?;
            }
            DataType::List(Arc::new(Field::new("item", element, true)))
        }
        _ => return None,
    })
}

fn list_array(values: &[Value], data_type: &DataType) -> RelResult<ArrayRef> {
    if values.is_empty() {
        return Ok(new_null_array(data_type, 0));
    }
    let scalars = values
        .iter()
        .map(|value| value_scalar(value, data_type))
        .collect::<RelResult<Vec<_>>>()?;
    Ok(ScalarValue::iter_to_array(scalars)?)
}

fn value_scalar(value: &Value, data_type: &DataType) -> RelResult<ScalarValue> {
    if matches!(value, Value::Null) {
        return Ok(ScalarValue::try_from(data_type)?);
    }
    let mismatch = || {
        RelError::Unsupported(format!(
            "list property value `{}` does not fit shadow type {data_type}",
            value.type_name()
        ))
    };
    Ok(match (data_type, value) {
        (DataType::Boolean, Value::Bool(value)) => ScalarValue::Boolean(Some(*value)),
        (DataType::Int64, Value::Int(value) | Value::Long(value)) => {
            ScalarValue::Int64(Some(*value))
        }
        (DataType::Float64, Value::Float(value)) => ScalarValue::Float64(Some(*value)),
        (DataType::Utf8, Value::String(value)) => ScalarValue::Utf8(Some(value.clone())),
        (DataType::List(field), Value::List(items)) => {
            let element = field.data_type();
            let items = items
                .iter()
                .map(|item| value_scalar(item, element))
                .collect::<RelResult<Vec<_>>>()?;
            ScalarValue::List(ScalarValue::new_list_nullable(&items, element))
        }
        _ => return Err(mismatch()),
    })
}

impl LoweringContext<'_> {
    /// Lower an expression used as a list operand, preferring a property's
    /// native list shadow over its display text.
    pub(super) fn lower_list_operand(&self, plan: &LogicalPlan, expr: &IrExpr) -> RelResult<Expr> {
        match native_list_property(plan, expr) {
            Some(native) => Ok(native),
            None if is_encoded_property(plan, expr) => Err(RelError::Unsupported(
                "list operation over an encoded property without a uniform native list type".into(),
            )),
            None => self.lower_expr(plan, expr),
        }
    }
}

/// Kuzu `list_slice` over a string: 1-based inclusive bounds, negative bounds
/// counted from the end, and null bounds selecting the whole side.
pub(super) fn string_slice_expr(text: Expr, start: Expr, end: Expr) -> Expr {
    let text = cast_utf8(text);
    let length = Expr::Cast(Cast::new(
        Box::new(df_unicode::character_length(text.clone())),
        DataType::Int64,
    ));
    let start = Expr::Cast(Cast::new(Box::new(start), DataType::Int64));
    let end = Expr::Cast(Cast::new(Box::new(end), DataType::Int64));
    let first = Expr::Case(Case::new(
        None,
        vec![
            (Box::new(start.clone().is_null()), Box::new(lit(1_i64))),
            (
                Box::new(binary(start.clone(), BinaryOp::Lt, lit(0_i64))),
                Box::new(binary(
                    binary(length.clone(), BinaryOp::Add, start.clone()),
                    BinaryOp::Add,
                    lit(1_i64),
                )),
            ),
        ],
        Some(Box::new(start)),
    ));
    let first = df_core::greatest(vec![first, lit(1_i64)]);
    let last = Expr::Case(Case::new(
        None,
        vec![
            (Box::new(end.clone().is_null()), Box::new(length.clone())),
            (
                Box::new(binary(end.clone(), BinaryOp::Lt, lit(0_i64))),
                Box::new(binary(
                    binary(length.clone(), BinaryOp::Add, end.clone()),
                    BinaryOp::Add,
                    lit(1_i64),
                )),
            ),
        ],
        Some(Box::new(end)),
    ));
    let last = df_core::least(vec![last, length]);
    Expr::Case(Case::new(
        None,
        vec![
            (
                Box::new(text.clone().is_null()),
                Box::new(lit(ScalarValue::Utf8(None))),
            ),
            (
                Box::new(binary(last.clone(), BinaryOp::Lt, first.clone())),
                Box::new(lit("")),
            ),
        ],
        Some(Box::new(df_unicode::substring(
            text,
            first.clone(),
            binary(
                binary(last, BinaryOp::Sub, first),
                BinaryOp::Add,
                lit(1_i64),
            ),
        ))),
    ))
}

/// A property stored as encoded structured text (list, map, union). Its
/// display text must never be treated as a string value by list operators.
pub(super) fn is_encoded_property(plan: &LogicalPlan, expr: &IrExpr) -> bool {
    matches!(expr, IrExpr::Property { binding, name, .. }
        if has_exact_col(plan, &union_tag_col(binding, name)))
}

/// Operands for list append/prepend over a non-literal list. A native list
/// shadow is used only when the inserted item has the list's element type;
/// otherwise both sides keep the display-text contract.
pub(super) fn lower_list_insert_operands(
    ctx: &LoweringContext<'_>,
    plan: &LogicalPlan,
    items: &IrExpr,
    item: &IrExpr,
) -> RelResult<(Expr, Expr)> {
    if let Some(native) = native_list_property(plan, items) {
        let element = match native.get_type(plan.schema())? {
            DataType::List(field) => Some(field.data_type().clone()),
            _ => None,
        };
        let item_expr = if matches!(item, IrExpr::List(_)) {
            ctx.lower_native_list(plan, item)
        } else {
            ctx.lower_list_operand(plan, item)
        };
        if let (Some(element), Ok(item_expr)) = (element, item_expr) {
            let item_type = item_expr.get_type(plan.schema())?;
            if item_type == element || item_type == DataType::Null {
                return Ok((native, item_expr));
            }
        }
    }
    Ok((ctx.lower_expr(plan, items)?, ctx.lower_expr(plan, item)?))
}

/// An explicit SELECT and CTE keep scalar consumers outside UNNEST.
pub(super) fn unnest_scope(plan: LogicalPlan, name: String) -> RelResult<LogicalPlan> {
    let mut columns = existing_columns_by_name(&plan, &BTreeSet::new());
    columns.push(lit(1_i64).alias(format!("{name}_guard")));
    Ok(LogicalPlanBuilder::from(plan)
        .project(columns)?
        .alias(name)?
        .build()?)
}

/// DataFusion's UNNEST unparser requires a Projection as its direct input.
/// Construct it directly so the builder does not elide an identity projection.
pub(super) fn unnest_input(plan: LogicalPlan) -> RelResult<LogicalPlan> {
    let columns = existing_columns_by_name(&plan, &BTreeSet::new());
    Ok(LogicalPlan::Projection(
        datafusion::logical_expr::logical_plan::Projection::try_new(columns, Arc::new(plan))?,
    ))
}

/// SQL UNNEST drops null and empty lists regardless of DataFusion's
/// preserve_nulls option. Supply a typed null element to retain the row.
pub(super) fn outer_list(list: Expr, data_type: &DataType) -> RelResult<Expr> {
    let element = match data_type {
        DataType::List(field) | DataType::LargeList(field) | DataType::FixedSizeList(field, _) => {
            field.data_type()
        }
        _ => return Err(RelError::Unsupported("outer UNNEST requires a list".into())),
    };
    let empty = df_core::coalesce(vec![
        datafusion::functions_nested::expr_fn::array_length(list.clone()),
        lit(0_i64),
    ])
    .eq(lit(0_i64));
    let singleton = datafusion::functions_nested::expr_fn::make_array(vec![lit(
        ScalarValue::try_from(element)?,
    )]);
    Ok(Expr::Case(Case::new(
        None,
        vec![(Box::new(empty), Box::new(singleton))],
        Some(Box::new(list)),
    )))
}
