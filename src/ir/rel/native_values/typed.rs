//! Small scalar kernels over typed Arrow arguments. No expression parser, JSON
//! row transport, or query execution is involved in these functions.
use super::*;
use arrow::array::{Array, ArrayRef, StringArray, StructArray};
use arrow::buffer::NullBuffer;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Output {
    Value,
    Boolean,
    Integer,
    Text,
    List,
    Indices,
}
impl Output {
    pub fn data_type(self) -> DataType {
        match self {
            Self::Value => value_type(),
            Self::Boolean => DataType::Boolean,
            Self::Integer => DataType::Int64,
            Self::Text => DataType::Utf8,
            Self::Indices => DataType::List(Arc::new(Field::new("item", DataType::Int64, true))),
            Self::List => DataType::List(Arc::new(Field::new("item", value_type(), true))),
        }
    }
}

// This inventory is deliberately semantic: each SQL function denotes one
// operation. Callers never supply a program or an operation name as an argument.
pub const CALLS: &[&str] = &[
    "abs",
    "cypher_coalesce",
    "cypher_convert.tointeger",
    "cypher_convert.tostring",
    "cypher_convert.tofloat",
    "cypher_convert.toboolean",
    "cypher_eq",
    "cypher_gt",
    "cypher_gte",
    "cypher_has_label",
    "cypher_in",
    "cypher_labels",
    "cypher_live_property",
    "cypher_lt",
    "cypher_lte",
    "cypher_merge_valid",
    "cypher_neq",
    "cypher_nodes",
    "cypher_order_key",
    "cypher_path",
    "cypher_slice_bound",
    "cypher_percentile_fraction",
    "cypher_property_map",
    "cypher_property_value",
    "cypher_range",
    "cypher_relationship_list",
    "cypher_relationships",
    "cypher_subscript",
    "cypher_temporal.date",
    "cypher_temporal.datetime",
    "cypher_temporal.duration",
    "cypher_temporal.duration.inseconds",
    "cypher_temporal.localdatetime",
    "cypher_temporal.localtime",
    "cypher_temporal.time",
    "cypher_type",
    "endNode",
    "head",
    "keys",
    "last",
    "length",
    "map",
    "mod",
    "path_append",
    "path_extend_after",
    "path_from",
    "path_or_self",
    "properties",
    "property",
    "recursive_relationship_path",
    "reverse",
    "size",
    "split",
    "startNode",
    "tail",
    "toLower",
    "typeof",
];
pub const AGGREGATES: &[(&str, AggKind)] = &[
    ("aggregate_sum", AggKind::Sum),
    ("aggregate_sum_or_zero", AggKind::SumOrZero),
    ("aggregate_avg", AggKind::Avg),
    ("aggregate_avg_or_zero", AggKind::AvgOrZero),
    ("aggregate_avg_or_null", AggKind::AvgOrNull),
    ("aggregate_min", AggKind::Min),
    ("aggregate_min_or_null", AggKind::MinOrNull),
    ("aggregate_max", AggKind::Max),
    ("aggregate_max_or_null", AggKind::MaxOrNull),
    ("aggregate_stdev", AggKind::StDev),
    ("aggregate_stdevp", AggKind::StDevP),
    ("aggregate_percentile_cont", AggKind::PercentileCont),
    ("aggregate_percentile_disc", AggKind::PercentileDisc),
    ("aggregate_collect_rows", AggKind::CollectRows),
    ("aggregate_collect_non_null", AggKind::CollectNonNull),
    ("aggregate_collect_traversers", AggKind::CollectTraversers),
];
pub fn operations() -> impl Iterator<Item = &'static str> {
    CALLS
        .iter()
        .chain(INTERNAL)
        .copied()
        .chain(AGGREGATES.iter().map(|(name, _)| *name))
}
pub const INTERNAL: &[&str] = &[
    "argument_valid",
    "validate_argument",
    "temporal_decode",
    "order_indices",
    "string_starts_with",
    "string_ends_with",
    "string_contains",
    "pack",
    "element",
    "key",
    "items",
    "list",
    "quantifier_list",
    "boolean",
    "add",
    "subtract",
    "multiply",
    "divide",
];
pub fn sql_name(operation: &str) -> String {
    format!(
        "orchid_cypher_{}",
        operation
            .strip_prefix("cypher_")
            .unwrap_or(operation)
            .replace('.', "_")
            .to_ascii_lowercase()
    )
}
pub fn output(operation: &str) -> Output {
    match operation {
        "order_indices" => Output::Indices,
        "key" => Output::Text,
        "argument_valid" | "string_starts_with" | "string_ends_with" | "string_contains"
        | "boolean" | "cypher_eq" | "cypher_neq" | "cypher_lt" | "cypher_lte" | "cypher_gt"
        | "cypher_gte" | "cypher_in" | "cypher_has_label" | "cypher_merge_valid" => Output::Boolean,
        "cypher_slice_bound" | "size" | "length" => Output::Integer,
        "items"
        | "list"
        | "quantifier_list"
        | "cypher_nodes"
        | "cypher_relationships"
        | "cypher_relationship_list" => Output::List,
        _ => Output::Value,
    }
}
pub fn operation(sql: &str) -> Option<&'static str> {
    operations().find(|name| sql_name(name) == sql)
}
pub(in crate::ir::rel) fn call(operation: &str, mut args: Vec<Expr>) -> Expr {
    if operation.starts_with("cypher_temporal.") {
        // The SQL engine supplies one statement/transaction clock, so sibling
        // temporal calls never observe different host wall-clock instants.
        args.insert(0, datafusion::functions::datetime::expr_fn::now());
    }
    ScalarUDF::from(NativeFunction {
        name: sql_name(operation),
        result: output(operation).data_type(),
        signature: if args.is_empty() {
            Signature::nullary(Volatility::Immutable)
        } else {
            Signature::any(args.len(), Volatility::Immutable)
        },
    })
    .call(args)
}

fn scalar(value: ScalarValue, context: &mut Context) -> Result<Value, String> {
    if value.is_null() {
        return Ok(Value::Null);
    }
    match value {
        ScalarValue::Decimal256(Some(n), 39, 0) => {
            let text = n.to_string();
            Ok(match text.parse::<i64>() {
                Ok(n) => Value::Long(n),
                Err(_) => Value::BigInt(text.parse().map_err(|_| "Invalid HUGEINT")?),
            })
        }
        value @ (ScalarValue::TimestampSecond(..)
        | ScalarValue::TimestampMillisecond(..)
        | ScalarValue::TimestampMicrosecond(..)
        | ScalarValue::TimestampNanosecond(..)) => {
            let (ticks, scale, zoned) = match value {
                ScalarValue::TimestampSecond(Some(n), zone) => (n, 1_i64, zone.is_some()),
                ScalarValue::TimestampMillisecond(Some(n), zone) => (n, 1000, zone.is_some()),
                ScalarValue::TimestampMicrosecond(Some(n), zone) => (n, 1_000_000, zone.is_some()),
                ScalarValue::TimestampNanosecond(Some(n), zone) => {
                    (n, 1_000_000_000, zone.is_some())
                }
                _ => unreachable!(),
            };
            let instant = chrono::DateTime::from_timestamp(
                ticks.div_euclid(scale),
                (ticks.rem_euclid(scale) * (1_000_000_000 / scale)) as u32,
            )
            .ok_or("Timestamp exceeds Cypher temporal range")?;
            Ok(Value::Temporal(if zoned {
                crate::ir::temporal::TemporalValue::DateTime(instant.fixed_offset(), None)
            } else {
                crate::ir::temporal::TemporalValue::LocalDateTime(instant.naive_utc())
            }))
        }
        ScalarValue::Struct(array) if is_value(array.data_type()) => {
            let strings = array
                .column(0)
                .as_any()
                .downcast_ref::<StringArray>()
                .ok_or("Invalid value carrier")?;
            if strings.is_null(0) {
                Ok(Value::Null)
            } else {
                context.decode(strings.value(0))
            }
        }
        ScalarValue::List(array) => list(array.value(0), context),
        ScalarValue::LargeList(array) => list(array.value(0), context),
        ScalarValue::FixedSizeList(array) => list(array.value(0), context),
        value if crate::ir::value::scalar_semantic_value(&value).is_some() => {
            Ok(crate::ir::value::scalar_semantic_value(&value).unwrap())
        }
        ScalarValue::Struct(array) => {
            let mut fields = array
                .fields()
                .iter()
                .zip(array.columns())
                .map(|(field, column)| {
                    Ok((
                        field.name().clone(),
                        scalar(
                            ScalarValue::try_from_array(column, 0).map_err(|e| e.to_string())?,
                            context,
                        )?,
                    ))
                })
                .collect::<Result<BTreeMap<_, _>, String>>()?;
            if fields.len() == 1 {
                if let Some(descriptor) = fields.remove(elements::FIELD) {
                    return context.attach(descriptor);
                }
                if let Some(Value::String(text)) = fields.get(FIELD) {
                    return context.decode(text);
                }
                for (domain, edge) in [
                    ("__orchiddb_domain_graph_node", false),
                    ("__orchiddb_domain_graph_edge", true),
                ] {
                    if let Some(descriptor) = fields.remove(domain) {
                        if descriptor == Value::Null {
                            return Ok(Value::Null);
                        }
                        let Value::Map(mut descriptor) = descriptor else {
                            return Err("Invalid graph reference".into());
                        };
                        descriptor.insert(
                            "kind".into(),
                            Value::String(if edge { "edge" } else { "node" }.into()),
                        );
                        descriptor.insert("properties".into(), Value::Map(BTreeMap::new()));
                        return context.attach(Value::Map(descriptor));
                    }
                }
                if let Some((domain, payload)) = fields.first_key_value()
                    && domain.starts_with("__orchiddb_domain_cypher_temporal")
                {
                    return match payload {
                        Value::Null | Value::Temporal(_) => Ok(payload.clone()),
                        Value::String(text) => crate::ir::temporal::TemporalValue::decode(text)
                            .or_else(|_| crate::ir::temporal::parse("date", text))
                            .map(Value::Temporal),
                        _ => Err("Invalid temporal domain payload".into()),
                    };
                }
            }
            Ok(Value::Map(fields))
        }
        value => Err(format!(
            "Unsupported typed scalar input {}",
            value.data_type()
        )),
    }
}
fn list(array: ArrayRef, context: &mut Context) -> Result<Value, String> {
    (0..array.len())
        .map(|row| {
            scalar(
                ScalarValue::try_from_array(&array, row).map_err(|e| e.to_string())?,
                context,
            )
        })
        .collect::<Result<Vec<_>, _>>()
        .map(Value::List)
}
fn values_array(values: &[Value], context: &Context) -> ArrayRef {
    let encoded = values
        .iter()
        .map(|value| context.encode(value))
        .collect::<Vec<_>>();
    let valid = NullBuffer::from(encoded.iter().map(Option::is_some).collect::<Vec<_>>());
    let DataType::Struct(fields) = value_type() else {
        unreachable!()
    };
    Arc::new(StructArray::new(
        fields,
        vec![Arc::new(StringArray::from(encoded))],
        Some(valid),
    ))
}

pub fn evaluate(operation: &str, arguments: &[ArrayRef], rows: usize) -> Result<ArrayRef, String> {
    if !operations().any(|name| name == operation) {
        return Err("Unknown typed scalar operation".into());
    }
    let mut values = Vec::with_capacity(rows);
    for row in 0..rows {
        let mut context = Context::default();
        let mut args = arguments
            .iter()
            .map(|array| {
                scalar(
                    ScalarValue::try_from_array(array, row).map_err(|e| e.to_string())?,
                    &mut context,
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
        if operation.starts_with("cypher_temporal.") {
            let clock = match args.first() {
                Some(Value::Temporal(crate::ir::temporal::TemporalValue::DateTime(clock, _))) => {
                    clock.with_timezone(&chrono::Utc)
                }
                Some(Value::Temporal(crate::ir::temporal::TemporalValue::LocalDateTime(clock))) => {
                    clock.and_utc()
                }
                _ => return Err("Temporal helper requires a SQL timestamp clock".into()),
            };
            context.graph.set_host_clocks(clock, clock);
            args.remove(0);
        }
        let value = match (operation, args.as_slice()) {
            ("string_starts_with" | "string_ends_with" | "string_contains", [target, pattern]) => {
                match (target, pattern) {
                    (Value::String(target), Value::String(pattern)) => {
                        Value::Bool(match operation {
                            "string_starts_with" => target.starts_with(pattern),
                            "string_ends_with" => target.ends_with(pattern),
                            _ => target.contains(pattern),
                        })
                    }
                    _ => Value::Null,
                }
            }
            (
                "order_indices",
                [
                    Value::List(rows),
                    Value::List(ascending),
                    Value::List(nulls_first),
                ],
            ) => {
                let mut indices = (0..rows.len()).collect::<Vec<_>>();
                let keys = rows
                    .iter()
                    .map(|row| match row {
                        Value::List(keys)
                            if keys.len() == ascending.len() && keys.len() == nulls_first.len() =>
                        {
                            Ok(keys)
                        }
                        _ => Err("Invalid sort keys"),
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                indices.sort_by(|&a, &b| {
                    for (i, (a, b)) in keys[a].iter().zip(keys[b]).enumerate() {
                        let order = match (a == &Value::Null, b == &Value::Null) {
                            (true, true) => std::cmp::Ordering::Equal,
                            (true, false) => {
                                if nulls_first[i] == Value::Bool(true) {
                                    std::cmp::Ordering::Less
                                } else {
                                    std::cmp::Ordering::Greater
                                }
                            }
                            (false, true) => {
                                if nulls_first[i] == Value::Bool(true) {
                                    std::cmp::Ordering::Greater
                                } else {
                                    std::cmp::Ordering::Less
                                }
                            }
                            _ => {
                                let order = crate::ir::runtime::scalar::compare_cypher_order(a, b);
                                if ascending[i] == Value::Bool(true) {
                                    order
                                } else {
                                    order.reverse()
                                }
                            }
                        };
                        if !order.is_eq() {
                            return order;
                        }
                    }
                    std::cmp::Ordering::Equal
                });
                Value::List(
                    indices
                        .into_iter()
                        .map(|i| Value::Int(i as i64 + 1))
                        .collect(),
                )
            }
            ("order_indices", [Value::Null, _, _]) => Value::List(vec![]),
            ("temporal_decode", [Value::String(value)]) => {
                Value::Temporal(crate::ir::temporal::TemporalValue::decode(value)?)
            }
            ("temporal_decode", [value @ (Value::Null | Value::Temporal(_))]) => value.clone(),
            (
                "validate_argument" | "argument_valid",
                [value, Value::String(type_name), Value::Bool(nullable)],
            ) => {
                let field = crate::ir::procedures::ProcedureField {
                    name: String::new(),
                    type_name: type_name.clone(),
                    nullable: *nullable,
                };
                if !field.accepts(value) {
                    return Err(crate::ir::diagnostics::QueryExecutionError::from_error(
                        crate::ir::runtime::RuntimeError::Diagnosed {
                            code: crate::ir::diagnostics::RuntimeDiagnosis::InvalidType,
                            message: "Invalid procedure argument".into(),
                        },
                    )
                    .sql_error());
                }
                if operation == "argument_valid" {
                    Value::Bool(true)
                } else {
                    value.clone()
                }
            }
            ("pack", [value]) => value.clone(),
            ("element", [descriptor]) => context.attach(descriptor.clone())?,
            ("key", [value]) => Value::String(
                STANDARD
                    .encode(crate::ir::runtime::ops::distinct::encode_cypher_equivalence(value)),
            ),
            ("boolean", [value @ (Value::Bool(_) | Value::Null)]) => value.clone(),
            ("boolean", [_]) => return Err("Expected a boolean predicate".into()),
            ("list" | "quantifier_list", [Value::List(items) | Value::Path(items)]) => {
                Value::List(items.clone())
            }
            ("list" | "quantifier_list", [Value::Null]) => Value::Null,
            ("quantifier_list", [_]) => Value::Null,
            ("list", [_]) => {
                return Err(crate::ir::diagnostics::QueryExecutionError::from_error(
                    crate::ir::runtime::RuntimeError::Type(
                        "List expression requires a list".into(),
                    ),
                )
                .sql_error());
            }
            ("items", [Value::List(items)]) => Value::List(items.clone()),
            ("items", [Value::Null]) => Value::List(vec![]),
            ("items", [value]) => Value::List(vec![value.clone()]),
            ("add" | "subtract" | "multiply" | "divide", [lhs, rhs]) => {
                let op = match operation {
                    "add" => BinaryOp::Add,
                    "subtract" => BinaryOp::Sub,
                    "multiply" => BinaryOp::Mul,
                    _ => BinaryOp::Div,
                };
                crate::ir::runtime::expr::arithmetic(op, lhs, rhs).map_err(|e| {
                    crate::ir::diagnostics::QueryExecutionError::from_error(e).sql_error()
                })?
            }
            (_, [input, Value::Bool(distinct)])
                if AGGREGATES.iter().any(|(name, _)| *name == operation) =>
            {
                let kind = AGGREGATES
                    .iter()
                    .find(|(name, _)| *name == operation)
                    .unwrap()
                    .1;
                let items = match input {
                    Value::Null => vec![],
                    Value::List(items) => items.clone(),
                    _ => return Err("Expected aggregate list".into()),
                };
                let arg = if matches!(kind, AggKind::PercentileCont | AggKind::PercentileDisc) {
                    IrExpr::List(
                        (0..2)
                            .map(|index| IrExpr::Call {
                                name: "cypher_subscript".into(),
                                args: vec![IrExpr::binding("value"), IrExpr::Lit(Lit::Int(index))],
                            })
                            .collect(),
                    )
                } else {
                    IrExpr::binding("value")
                };
                let aggregate = AggCall {
                    kind,
                    alias: "result".into(),
                    arg: Some(arg),
                    distinct: *distinct,
                };
                let rows = items
                    .into_iter()
                    .map(|value| KernelRow::new().with("value", value))
                    .collect();
                let rows =
                    crate::ir::runtime::ops::aggregate::aggregate_op_with_cypher_equivalence(
                        &[],
                        &[aggregate],
                        rows,
                        &context.graph,
                        true,
                    )
                    .map_err(|e| {
                        crate::ir::diagnostics::QueryExecutionError::from_error(e).sql_error()
                    })?;
                rows.first()
                    .map(|row| row.get("result"))
                    .unwrap_or(Value::Null)
            }
            _ if CALLS.contains(&operation) => {
                crate::ir::runtime::scalar::eval_call(operation, args, &context.graph).map_err(
                    |e| crate::ir::diagnostics::QueryExecutionError::from_error(e).sql_error(),
                )?
            }
            _ => return Err(format!("Invalid {operation} arguments")),
        };
        // Encode each row before releasing its detached element context. Two
        // rows can contain snapshots of the same identity with different data.
        let result = match (output(operation), value) {
            (kind, Value::Null) => {
                ScalarValue::try_from(&kind.data_type()).map_err(|e| e.to_string())?
            }
            (Output::Value, value) => {
                ScalarValue::try_from_array(&values_array(&[value], &context), 0)
                    .map_err(|e| e.to_string())?
            }
            (Output::Boolean, Value::Bool(value)) => ScalarValue::Boolean(Some(value)),
            (Output::Integer, value) => {
                ScalarValue::Int64(Some(value.as_i64().ok_or("Expected integer output")?))
            }
            (Output::Text, Value::String(value)) => ScalarValue::Utf8(Some(value)),
            (Output::Indices, Value::List(items)) => ScalarValue::List(ScalarValue::new_list(
                &items
                    .iter()
                    .map(|value| ScalarValue::Int64(value.as_i64()))
                    .collect::<Vec<_>>(),
                &DataType::Int64,
                true,
            )),
            (Output::List, Value::List(items)) => {
                ScalarValue::List(Arc::new(arrow::array::ListArray::new(
                    Arc::new(Field::new("item", value_type(), true)),
                    arrow::buffer::OffsetBuffer::new(
                        vec![0, i32::try_from(items.len()).map_err(|_| "List too large")?].into(),
                    ),
                    values_array(&items, &context),
                    None,
                )))
            }
            _ => return Err("Typed scalar violated its result contract".into()),
        };
        values.push(result);
    }
    if values.is_empty() {
        Ok(arrow::array::new_empty_array(
            &output(operation).data_type(),
        ))
    } else {
        ScalarValue::iter_to_array(values).map_err(|e| e.to_string())
    }
}

fn packed(plan: &LogicalPlan, value: Expr) -> RelResult<Expr> {
    Ok(if is_value(&value.get_type(plan.schema())?) {
        value
    } else {
        call("pack", vec![value])
    })
}
pub(super) fn lower(
    context: &LoweringContext<'_>,
    plan: &LogicalPlan,
    expr: &IrExpr,
) -> RelResult<Expr> {
    let lower = |expr| -> RelResult<Expr> {
        let value = context.lower_expr(plan, expr)?;
        let ty = value.get_type(plan.schema())?;
        Ok(
            if crate::ir::temporal::transport::is_type(&ty)
                && !crate::ir::temporal::transport::is_date_type(&ty)
            {
                call("temporal_decode", vec![value])
            } else {
                value
            },
        )
    };
    let boolean = |expr| -> RelResult<Expr> {
        let value = lower(expr)?;
        Ok(if is_value(&value.get_type(plan.schema())?) {
            call("boolean", vec![value])
        } else {
            value
        })
    };
    match expr {
        IrExpr::Binding(binding) => {
            if let Some(column) = resolve_column_name(plan, binding) {
                return Ok(col_exact(column));
            }
            if has_binding_shape(plan, binding).is_some() {
                return context.native_element(plan, binding);
            }
            if binding == "__path" || binding == "__path_labels" {
                return Ok(lit(ScalarValue::Null));
            }
        }
        IrExpr::Lit(value) => return Ok(lit_to_expr(value)),
        IrExpr::Call { name, args } if name == "contains_fn" && args.len() == 2 => {
            return Ok(call(
                "string_contains",
                args.iter().map(lower).collect::<RelResult<Vec<_>>>()?,
            ));
        }
        IrExpr::Call { name, args } if CALLS.contains(&name.as_str()) => {
            // CASE/COALESCE remain SQL operators so untaken branches cannot
            // evaluate throwing or volatile scalar kernels eagerly.
            if name == "cypher_coalesce" {
                return Ok(df_core::coalesce(
                    args.iter()
                        .map(|arg| packed(plan, lower(arg)?))
                        .collect::<RelResult<Vec<_>>>()?,
                ));
            }
            return Ok(call(
                name,
                args.iter().map(lower).collect::<RelResult<Vec<_>>>()?,
            ));
        }
        IrExpr::List(items) => {
            return Ok(datafusion::functions_nested::expr_fn::make_array(
                items
                    .iter()
                    .map(|item| packed(plan, lower(item)?))
                    .collect::<RelResult<Vec<_>>>()?,
            ));
        }
        IrExpr::Property { binding, name, .. } => {
            return Ok(call(
                "cypher_subscript",
                vec![lower(&IrExpr::Binding(binding.clone()))?, lit(name.clone())],
            ));
        }
        IrExpr::Binary { op, lhs, rhs } => {
            if matches!(op, BinaryOp::And | BinaryOp::Or) {
                return Ok(super::super::binary(boolean(lhs)?, *op, boolean(rhs)?));
            }
            let operation = match op {
                BinaryOp::Add => "add",
                BinaryOp::Sub => "subtract",
                BinaryOp::Mul => "multiply",
                BinaryOp::Div => "divide",
                BinaryOp::Eq => "cypher_eq",
                BinaryOp::Neq => "cypher_neq",
                BinaryOp::Lt => "cypher_lt",
                BinaryOp::Lte => "cypher_lte",
                BinaryOp::Gt => "cypher_gt",
                BinaryOp::Gte => "cypher_gte",
                _ => unreachable!(),
            };
            return Ok(call(operation, vec![lower(lhs)?, lower(rhs)?]));
        }
        IrExpr::StringPredicate {
            op,
            target,
            pattern,
        } => {
            return Ok(call(
                match op {
                    StringOp::StartsWith => "string_starts_with",
                    StringOp::EndsWith => "string_ends_with",
                    StringOp::Contains => "string_contains",
                },
                vec![lower(target)?, lower(pattern)?],
            ));
        }
        IrExpr::Not(inner) => return Ok(Expr::Not(Box::new(boolean(inner)?))),
        IrExpr::IsNull(inner) => return Ok(lower(inner)?.is_null()),
        IrExpr::IsNotNull(inner) => return Ok(lower(inner)?.is_not_null()),
        IrExpr::Case { arms, otherwise } => {
            let arms = arms
                .iter()
                .map(|(condition, value)| {
                    Ok((
                        Box::new(boolean(condition)?),
                        Box::new(packed(plan, lower(value)?)?),
                    ))
                })
                .collect::<RelResult<Vec<_>>>()?;
            let otherwise = otherwise
                .as_ref()
                .map(|value| packed(plan, lower(value)?).map(Box::new))
                .transpose()?;
            return Ok(Expr::Case(Case::new(None, arms, otherwise)));
        }
        IrExpr::ListTransform { .. } | IrExpr::ListFilter { .. } | IrExpr::Quantifier { .. } => {
            return context.lower_sql_lambda(plan, expr);
        }
        _ => {}
    }
    Err(RelError::Unsupported(format!(
        "No typed SQL lowering for {expr:?}"
    )))
}
