//! Constant evaluation, casts, and language-specific value rendering.

use super::*;

pub(super) fn lit_to_expr(value: &Lit) -> Expr {
    match value {
        Lit::Null => lit(ScalarValue::Null),
        Lit::Bool(value) => lit(*value),
        Lit::Int(value) => lit(*value),
        Lit::Float(value) => lit(*value),
        Lit::String(value) => lit(value.clone()),
    }
}

pub(super) fn value_literal_expr(value: &Value) -> RelResult<Expr> {
    match value {
        Value::Null => Ok(lit(ScalarValue::Null)),
        Value::Bool(value) => Ok(lit(*value)),
        Value::Byte(value) => Ok(lit(*value as i64)),
        Value::UInt8(value) => Ok(lit(*value as i64)),
        Value::Short(value) => Ok(lit(*value as i64)),
        Value::UInt16(value) => Ok(lit(*value as i64)),
        Value::Int(value) | Value::Long(value) => Ok(lit(*value)),
        Value::UInt32(value) => Ok(lit(*value as i64)),
        Value::UInt64(value) => i64::try_from(*value)
            .map(lit)
            .map_err(|_| RelError::Unsupported("UINT64 choose key overflows INT64".into())),
        Value::Float32(value) => Ok(lit(*value as f64)),
        Value::Float(value) => Ok(lit(*value)),
        Value::String(value) | Value::DateTime(value) => Ok(lit(value.clone())),
        other => Err(RelError::Unsupported(format!(
            "GraphChoose key type `{}` is not relationally lowered yet",
            other.type_name()
        ))),
    }
}

pub(super) fn lit_to_value(value: &Lit) -> Value {
    match value {
        Lit::Null => Value::Null,
        Lit::Bool(value) => Value::Bool(*value),
        Lit::Int(value) => Value::Int(*value),
        Lit::Float(value) => Value::Float(*value),
        Lit::String(value) => Value::String(value.clone()),
    }
}

/// Whether an IR expression can be evaluated without any row context
/// (modulo `bound` iteration variables introduced by list lambdas).
pub(super) fn expr_is_constant(expr: &IrExpr, bound: &[&str]) -> bool {
    match expr {
        IrExpr::Lit(_) => true,
        IrExpr::Binding(binding) => bound.contains(&binding.as_str()),
        IrExpr::List(items) => items.iter().all(|item| expr_is_constant(item, bound)),
        IrExpr::Binary { lhs, rhs, .. } => {
            expr_is_constant(lhs, bound) && expr_is_constant(rhs, bound)
        }
        IrExpr::Not(inner) | IrExpr::IsNull(inner) | IrExpr::IsNotNull(inner) => {
            expr_is_constant(inner, bound)
        }
        IrExpr::StringPredicate {
            target, pattern, ..
        } => expr_is_constant(target, bound) && expr_is_constant(pattern, bound),
        IrExpr::Case { arms, otherwise } => {
            arms.iter()
                .all(|(when, then)| expr_is_constant(when, bound) && expr_is_constant(then, bound))
                && otherwise
                    .as_deref()
                    .is_none_or(|expr| expr_is_constant(expr, bound))
        }
        IrExpr::ListTransform { list, item, map } => {
            expr_is_constant(list, bound) && {
                let mut inner = bound.to_vec();
                inner.push(item.as_str());
                expr_is_constant(map, &inner)
            }
        }
        IrExpr::ListFilter {
            list,
            item,
            predicate,
        } => {
            expr_is_constant(list, bound) && {
                let mut inner = bound.to_vec();
                inner.push(item.as_str());
                expr_is_constant(predicate, &inner)
            }
        }
        IrExpr::ListReduce {
            collection,
            accumulator,
            item,
            map,
        } => {
            expr_is_constant(collection, bound) && {
                let mut inner = bound.to_vec();
                inner.push(accumulator.as_str());
                inner.push(item.as_str());
                expr_is_constant(map, &inner)
            }
        }
        IrExpr::Call { name, args } => {
            constant_foldable_function(name) && args.iter().all(|arg| expr_is_constant(arg, bound))
        }
        _ => false,
    }
}

/// Functions safe to fold at lowering time: deterministic, side-effect
/// free, and not tied to internal traversal state.
pub(super) fn constant_foldable_function(name: &str) -> bool {
    let normalized = name.to_ascii_lowercase();
    if !crate::ir::interpreter::is_known_function(&normalized) || normalized.starts_with("__") {
        return false;
    }
    const DENY: &[&str] = &[
        "rand",
        "random",
        "uuid",
        "gen_random_uuid",
        "now",
        "nextval",
        "currval",
    ];
    if DENY.contains(&normalized.as_str()) {
        return false;
    }
    const DENY_PREFIX: &[&str] = &[
        "current_",
        "select_",
        "path_",
        "history_",
        "cypher_property",
    ];
    !DENY_PREFIX
        .iter()
        .any(|prefix| normalized.starts_with(prefix))
}

/// Kuzu-style engine errors ("Conversion exception: ...") are expected
/// outcomes for some cases; internal interpreter errors are not and should
/// fall back to relational lowering.
pub(super) fn looks_like_engine_error(message: &str) -> bool {
    const PREFIXES: &[&str] = &[
        "Conversion exception",
        "Overflow exception",
        "Runtime exception",
        "Binder exception",
        "Parser exception",
        "Catalog exception",
        "RuntimeError",
        "SyntaxError",
    ];
    PREFIXES.iter().any(|prefix| message.starts_with(prefix)) || message.contains(" exception:")
}

pub(super) fn constant_fold_result_expr(value: &Value, language: Language) -> Expr {
    match value {
        Value::Null => lit(ScalarValue::Utf8(None)),
        Value::Bool(value) => lit(*value),
        Value::Byte(value) => lit(*value as i64),
        Value::UInt8(value) => lit(*value as i64),
        Value::Short(value) => lit(*value as i64),
        Value::UInt16(value) => lit(*value as i64),
        Value::Int(value) | Value::Long(value) => lit(*value),
        Value::UInt32(value) => lit(*value as i64),
        Value::UInt64(value) => match i64::try_from(*value) {
            Ok(value) => lit(value),
            Err(_) => lit(value.to_string()),
        },
        Value::Float32(value) => lit(*value as f64),
        Value::Float(value) => lit(*value),
        Value::String(value) | Value::DateTime(value) => lit(value.clone()),
        Value::BigInt(value) => match value.to_i64() {
            Some(value) => lit(value),
            None => lit(value.to_string()),
        },
        other => constant_result_expr(other, language, literal_collection_context(language)),
    }
}

pub(super) fn constant_value_expr(expr: &IrExpr) -> RelResult<Option<Value>> {
    match expr {
        IrExpr::Lit(value) => Ok(Some(lit_to_value(value))),
        IrExpr::List(items) => {
            let mut values = Vec::with_capacity(items.len());
            for item in items {
                let Some(value) = constant_value_expr(item)? else {
                    return Ok(None);
                };
                values.push(value);
            }
            Ok(Some(Value::List(values)))
        }
        IrExpr::Binary {
            op: BinaryOp::Sub,
            lhs,
            rhs,
        } if matches!(lhs.as_ref(), IrExpr::Lit(Lit::Int(0))) => Ok(constant_value_expr(rhs)?
            .and_then(|value| match value {
                Value::Byte(value) => value.checked_neg().map(Value::Byte),
                Value::Short(value) => value.checked_neg().map(Value::Short),
                Value::Int(value) => value.checked_neg().map(Value::Int),
                Value::Long(value) => value.checked_neg().map(Value::Long),
                Value::Float32(value) => Some(Value::Float32(-value)),
                Value::Float(value) => Some(Value::Float(-value)),
                Value::BigInt(value) => Some(Value::BigInt(-value)),
                Value::BigDecimal(value) => Some(Value::BigDecimal(-value)),
                _ => None,
            })),
        IrExpr::Call { name, args } if name.eq_ignore_ascii_case("range") => {
            Ok(Some(Value::List(constant_range_values(args)?)))
        }
        IrExpr::Call { name, args } if name == "integer_literal" && args.len() == 1 => {
            let Some(text) = integer_literal_text(&args[0]) else {
                return Ok(None);
            };
            if let Ok(value) = text.replace('_', "").parse::<i64>() {
                Ok(Some(Value::Int(value)))
            } else {
                let value = BigInt::from_str(&text.replace('_', "")).map_err(|_| {
                    RelError::Unsupported(format!("invalid integer literal `{text}`"))
                })?;
                Ok(Some(Value::BigInt(value)))
            }
        }
        IrExpr::Call { name, args } if is_date_constructor(name) => {
            constant_temporal_value(name, args)
        }
        IrExpr::Call { name, args } if is_cast_function(name, args) => {
            let Some(value) = constant_value_expr(
                args.first()
                    .ok_or_else(|| RelError::Unsupported(format!("{name} arity")))?,
            )?
            else {
                return Ok(None);
            };
            let Some(target) = constant_cast_target(name, args)? else {
                return Ok(None);
            };
            constant_cast_value(&value, &target)
        }
        IrExpr::Call { name, args } if name == "map" => constant_cypher_map(args),
        _ => Ok(None),
    }
}

pub(super) fn union_constructor_field(args: &[IrExpr]) -> RelResult<(&str, &IrExpr)> {
    let [IrExpr::Call { name, args }] = args else {
        return Err(RelError::Unsupported(
            "union_value expects one named argument".into(),
        ));
    };
    if name != "map" {
        return Err(RelError::Unsupported(
            "union_value expects one named argument".into(),
        ));
    }
    let [IrExpr::Lit(Lit::String(tag)), value] = args.as_slice() else {
        return Err(RelError::Unsupported(
            "union_value expects one named argument".into(),
        ));
    };
    Ok((tag, value))
}

pub(super) fn constant_cast_target(name: &str, args: &[IrExpr]) -> RelResult<Option<String>> {
    let normalized = normalize_function_name(name);
    if normalized == "cast" {
        let Some(target) = args.get(1) else {
            return Err(RelError::Unsupported("cast arity".into()));
        };
        let Some(Value::String(target)) = constant_value_expr(target)? else {
            return Ok(None);
        };
        return Ok(Some(target));
    }
    Ok(Some(
        cast_target_from_function_name(&normalized)?.to_string(),
    ))
}

pub(super) fn constant_cast_value(value: &Value, target: &str) -> RelResult<Option<Value>> {
    if matches!(value, Value::Null) {
        return Ok(Some(Value::Null));
    }
    let normalized = target
        .trim()
        .trim_matches('"')
        .to_ascii_uppercase()
        .replace(' ', "");
    if normalized.contains('[') {
        return Ok(Some(value.clone()));
    }
    let out = match normalized.as_str() {
        "BOOL" | "BOOLEAN" => match value {
            Value::Bool(value) => Value::Bool(*value),
            Value::String(value) if value.eq_ignore_ascii_case("true") => Value::Bool(true),
            Value::String(value) if value.eq_ignore_ascii_case("false") => Value::Bool(false),
            _ => return Ok(None),
        },
        "INT8" => Value::Byte(
            cast_bigint_range(value, -128_i128, 127_i128)?
                .to_i64()
                .unwrap() as i8,
        ),
        "INT16" => Value::Short(
            cast_bigint_range(value, -32768_i128, 32767_i128)?
                .to_i64()
                .unwrap() as i16,
        ),
        "INT32" => Value::Int(
            cast_bigint_range(value, i32::MIN as i128, i32::MAX as i128)?
                .to_i64()
                .unwrap(),
        ),
        "INT64" | "SERIAL" => Value::Long(
            cast_bigint_range(value, i64::MIN as i128, i64::MAX as i128)?
                .to_i64()
                .unwrap(),
        ),
        "INT128" => Value::BigInt(
            value_to_bigint(value)
                .ok_or_else(|| RelError::Unsupported("constant INT128 cast".into()))?,
        ),
        "UINT8" => Value::UInt8(
            cast_bigint_range(value, 0_i128, u8::MAX as i128)?
                .to_u8()
                .unwrap(),
        ),
        "UINT16" => Value::UInt16(
            cast_bigint_range(value, 0_i128, u16::MAX as i128)?
                .to_u16()
                .unwrap(),
        ),
        "UINT32" => Value::UInt32(
            cast_bigint_range(value, 0_i128, u32::MAX as i128)?
                .to_u32()
                .unwrap(),
        ),
        "UINT64" => Value::UInt64(
            cast_bigint_range_big_max(value, BigInt::from(0), BigInt::from(u64::MAX))?
                .to_u64()
                .unwrap(),
        ),
        "UINT128" => {
            let value = value_to_bigint(value)
                .ok_or_else(|| RelError::Unsupported("constant UINT128 cast".into()))?;
            if value < BigInt::from(0) {
                return Ok(None);
            }
            Value::UInt128(value)
        }
        "FLOAT" => Value::Float32(
            value_to_f64(value)
                .ok_or_else(|| RelError::Unsupported("constant FLOAT cast".into()))?
                as f32,
        ),
        "DOUBLE" | "FLOAT64" => Value::Float(
            value_to_f64(value)
                .ok_or_else(|| RelError::Unsupported("constant DOUBLE cast".into()))?,
        ),
        "STRING" | "VARCHAR" => Value::String(cypher_plain_value(value)),
        "DATE" | "TIMESTAMP" | "TIMESTAMP_NS" | "TIMESTAMP_MS" | "TIMESTAMP_SEC"
        | "TIMESTAMP_S" | "TIMESTAMP_TZ" => match value {
            Value::String(value) | Value::DateTime(value) => Value::DateTime(value.clone()),
            _ => return Ok(None),
        },
        _ => return Ok(None),
    };
    Ok(Some(out))
}

pub(super) fn cast_bigint_range(value: &Value, min: i128, max: i128) -> RelResult<BigInt> {
    cast_bigint_range_big_max(value, BigInt::from(min), BigInt::from(max))
}

pub(super) fn cast_bigint_range_big_max(
    value: &Value,
    min: BigInt,
    max: BigInt,
) -> RelResult<BigInt> {
    let value = value_to_bigint(value)
        .ok_or_else(|| RelError::Unsupported("constant integer cast".into()))?;
    if value < min || value > max {
        return Err(RelError::Unsupported(
            "constant integer cast out of range".into(),
        ));
    }
    Ok(value)
}

pub(super) fn value_to_bigint(value: &Value) -> Option<BigInt> {
    match value {
        Value::Byte(value) => Some(BigInt::from(*value)),
        Value::UInt8(value) => Some(BigInt::from(*value)),
        Value::Short(value) => Some(BigInt::from(*value)),
        Value::UInt16(value) => Some(BigInt::from(*value)),
        Value::Int(value) | Value::Long(value) => Some(BigInt::from(*value)),
        Value::UInt32(value) => Some(BigInt::from(*value)),
        Value::UInt64(value) => Some(BigInt::from(*value)),
        Value::BigInt(value) | Value::UInt128(value) => Some(value.clone()),
        Value::BigDecimal(value) => value.to_i128().map(BigInt::from),
        Value::Bool(true) => Some(BigInt::from(1)),
        Value::Bool(false) => Some(BigInt::from(0)),
        Value::Float32(value) => Some(BigInt::from((*value as f64).round() as i64)),
        Value::Float(value) => Some(BigInt::from(value.round() as i64)),
        Value::String(value) => BigInt::from_str(value).ok(),
        _ => None,
    }
}

pub(super) fn value_to_f64(value: &Value) -> Option<f64> {
    match value {
        Value::Byte(value) => Some(*value as f64),
        Value::UInt8(value) => Some(*value as f64),
        Value::Short(value) => Some(*value as f64),
        Value::UInt16(value) => Some(*value as f64),
        Value::Int(value) | Value::Long(value) => Some(*value as f64),
        Value::UInt32(value) => Some(*value as f64),
        Value::UInt64(value) => Some(*value as f64),
        Value::Float32(value) => Some(*value as f64),
        Value::Float(value) => Some(*value),
        Value::BigInt(value) | Value::UInt128(value) => value.to_f64(),
        Value::BigDecimal(value) => value.to_f64(),
        Value::String(value) => value.parse().ok(),
        _ => None,
    }
}

/// Gremlin tagged text for a scalar expression, mirroring the
/// interpreter's `tagged_value` (`d[5].i`, `d[2.0].d`, raw strings,
/// `true`/`false`).
pub(super) fn gremlin_tagged_text_expr(expr: Expr, data_type: &DataType) -> Expr {
    match data_type {
        DataType::Boolean => Expr::Case(Case::new(
            None,
            vec![(Box::new(expr), Box::new(lit("true")))],
            Some(Box::new(lit("false"))),
        )),
        DataType::Int8 => concat_exprs(vec![lit("d["), cast_utf8(expr), lit("].b")]),
        DataType::Int16 => concat_exprs(vec![lit("d["), cast_utf8(expr), lit("].s")]),
        DataType::Int32 | DataType::Int64 => {
            concat_exprs(vec![lit("d["), cast_utf8(expr), lit("].i")])
        }
        DataType::Float32 => concat_exprs(vec![lit("d["), cast_utf8(expr), lit("].f")]),
        DataType::Float64 => concat_exprs(vec![lit("d["), cast_utf8(expr), lit("].d")]),
        _ => cast_utf8(expr),
    }
}

/// Render one property column as the text the interpreter's
/// `format_property_value` would produce.
pub(super) fn render_property_text_expr(column: Expr, data_type: &DataType) -> Expr {
    match data_type {
        DataType::Boolean => Expr::Case(Case::new(
            None,
            vec![(Box::new(column), Box::new(lit("True")))],
            Some(Box::new(lit("False"))),
        )),
        DataType::Float64 | DataType::Float32 => cast_utf8(Expr::Cast(Cast::new(
            Box::new(column),
            DataType::Decimal128(38, 6),
        ))),
        DataType::Utf8 | DataType::LargeUtf8 | DataType::Utf8View => column,
        _ => cast_utf8(column),
    }
}

pub(super) fn gremlin_element_display_expr(plan: &LogicalPlan, binding: &str) -> RelResult<Expr> {
    let Some(_) = has_binding_shape(plan, binding) else {
        return Err(RelError::Unsupported(format!(
            "binding `{binding}` is not an element binding"
        )));
    };
    Ok(string_concat(
        string_concat(col_exact(label_col(binding)), lit("#")),
        cast_utf8(col_exact(id_col(binding))),
    ))
}

#[derive(Clone, Copy)]
pub(super) enum DisplayContext {
    Scalar,
    Tagged,
}

pub(super) fn graph_values_display(value: &Value, language: Language) -> String {
    match language {
        Language::Cypher | Language::Gql => {
            rel_display_value(value, language, DisplayContext::Tagged)
        }
        Language::Gremlin => match value {
            Value::List(_) | Value::Map(_) | Value::Path(_) => format!("{value:?}"),
            _ => rel_display_value(value, language, DisplayContext::Tagged),
        },
        Language::Sparql => rel_display_value(value, language, DisplayContext::Tagged),
    }
}

pub(super) fn rel_display_value(
    value: &Value,
    language: Language,
    context: DisplayContext,
) -> String {
    match language {
        Language::Cypher | Language::Gql => match context {
            DisplayContext::Scalar => cypher_plain_value(value),
            DisplayContext::Tagged => tagged_value(value),
        },
        Language::Gremlin | Language::Sparql => tagged_value(value),
    }
}

pub(super) fn literal_collection_context(language: Language) -> DisplayContext {
    match language {
        Language::Cypher | Language::Gql => DisplayContext::Scalar,
        Language::Gremlin | Language::Sparql => DisplayContext::Tagged,
    }
}

pub(super) fn constant_result_expr(
    value: &Value,
    language: Language,
    context: DisplayContext,
) -> Expr {
    match value {
        Value::Null => lit(ScalarValue::Utf8(None)),
        _ => lit(rel_display_value(value, language, context)),
    }
}

pub(super) fn tagged_value(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(value) => value.to_string(),
        Value::Byte(value) => format!("d[{value}].b"),
        Value::UInt8(value) => format!("d[{value}].u8"),
        Value::Short(value) => format!("d[{value}].s"),
        Value::UInt16(value) => format!("d[{value}].u16"),
        Value::Int(value) => format!("d[{value}].i"),
        Value::UInt32(value) => format!("d[{value}].u32"),
        Value::Long(value) => format!("d[{value}].l"),
        Value::UInt64(value) => format!("d[{value}].u64"),
        Value::Float32(value) => format!("d[{value}].f"),
        Value::Float(value) => format!("d[{value}].d"),
        Value::BigInt(value) => format!("d[{value}].n"),
        Value::UInt128(value) => format!("d[{value}].u128"),
        Value::BigDecimal(value) => format!("d[{value}].m"),
        Value::DateTime(value) => format!("dt[{value}]"),
        Value::InternalId { table, offset } => format!("{table}:{offset}"),
        Value::String(value) => value.clone(),
        Value::Node { label, id } => format!("v[{label}#{id}]"),
        Value::Edge { rel_type, id, .. } => format!("e[{rel_type}#{id}]"),
        Value::List(items) | Value::Path(items) => {
            let prefix = if matches!(value, Value::Path(_)) {
                "p"
            } else {
                "l"
            };
            let parts = items.iter().map(tagged_value).collect::<Vec<_>>();
            format!("{prefix}[{}]", parts.join(","))
        }
        Value::Map(map) => {
            let parts = map
                .iter()
                .filter(|(key, _)| {
                    !key.starts_with("__")
                        && key.as_str() != STRUCT_ORDER_KEY
                        && key.as_str() != STRUCT_TYPES_KEY
                })
                .map(|(key, value)| {
                    format!(
                        "\"{}\":\"{}\"",
                        escape_debug_string(key),
                        tagged_value(value)
                    )
                })
                .collect::<Vec<_>>();
            format!("m[{{{}}}]", parts.join(","))
        }
    }
}

pub(super) fn cypher_plain_value(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::Bool(true) => "True".to_string(),
        Value::Bool(false) => "False".to_string(),
        Value::Byte(value) => value.to_string(),
        Value::UInt8(value) => value.to_string(),
        Value::Short(value) => value.to_string(),
        Value::UInt16(value) => value.to_string(),
        Value::Int(value) | Value::Long(value) => value.to_string(),
        Value::UInt32(value) => value.to_string(),
        Value::UInt64(value) => value.to_string(),
        Value::Float32(value) => cypher_float_text(*value as f64),
        Value::Float(value) => cypher_float_text(*value),
        Value::BigInt(value) | Value::UInt128(value) => value.to_string(),
        Value::BigDecimal(value) => value.to_string(),
        Value::DateTime(value) | Value::String(value) => value.clone(),
        Value::InternalId { table, offset } => format!("{table}:{offset}"),
        Value::Node { label, id } => format!("{label}#{id}"),
        Value::Edge { rel_type, id, .. } => format!("{rel_type}#{id}"),
        Value::List(items) => {
            let body = items.iter().map(cypher_plain_value).collect::<Vec<_>>();
            format!("[{}]", body.join(","))
        }
        Value::Path(items) => {
            let body = items.iter().map(cypher_plain_value).collect::<Vec<_>>();
            format!("[{}]", body.join(","))
        }
        Value::Map(map) => {
            if let Some(Value::List(entries)) = map.get("\u{0}kuzu_map_entries") {
                let body = entries
                    .iter()
                    .filter_map(|entry| {
                        let Value::List(pair) = entry else {
                            return None;
                        };
                        let [key, value] = pair.as_slice() else {
                            return None;
                        };
                        Some(format!(
                            "{}={}",
                            cypher_plain_value(key),
                            cypher_plain_value(value)
                        ))
                    })
                    .collect::<Vec<_>>();
                return format!("{{{}}}", body.join(", "));
            }
            // A union is carried as a tagged map but prints as its payload
            // alone: `CAST(127 AS UNION(a STRING, b INT64))` is `127`, not
            // `{b: 127}`. Mirrors `output::union_display_value`.
            if let (Some(Value::String(_)), Some(payload)) = (map.get("__tag"), map.get("__value"))
            {
                return cypher_plain_value(payload);
            }
            // Structs print in declaration order. The map is sorted, so
            // without the recorded order the fields come out alphabetized.
            let ordered_keys: Vec<String> = match map.get(STRUCT_ORDER_KEY) {
                Some(Value::List(items)) => items
                    .iter()
                    .filter_map(|item| match item {
                        Value::String(key) if map.contains_key(key) => Some(key.clone()),
                        _ => None,
                    })
                    .collect(),
                _ => map
                    .keys()
                    .filter(|key| {
                        !key.starts_with("__")
                            && key.as_str() != STRUCT_ORDER_KEY
                            && key.as_str() != STRUCT_TYPES_KEY
                    })
                    .cloned()
                    .collect(),
            };
            let body = ordered_keys
                .iter()
                .filter_map(|key| {
                    map.get(key)
                        .map(|value| format!("{key}: {}", cypher_plain_value(value)))
                })
                .collect::<Vec<_>>();
            format!("{{{}}}", body.join(", "))
        }
    }
}

pub(super) fn cypher_float_text(value: f64) -> String {
    if value.is_finite() {
        format!("{value:.6}")
    } else {
        value.to_string()
    }
}

pub(super) fn escape_debug_string(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

pub(super) fn visible_map_keys(map: &BTreeMap<String, Value>) -> Vec<String> {
    map.keys()
        .filter(|key| {
            key.as_str() != STRUCT_ORDER_KEY
                && key.as_str() != STRUCT_TYPES_KEY
                && !key.starts_with("__")
        })
        .cloned()
        .collect()
}

pub(super) fn display_for_list_to_string(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::Bool(true) => "True".to_string(),
        Value::Bool(false) => "False".to_string(),
        Value::Byte(value) => value.to_string(),
        Value::UInt8(value) => value.to_string(),
        Value::Short(value) => value.to_string(),
        Value::UInt16(value) => value.to_string(),
        Value::Int(value) | Value::Long(value) => value.to_string(),
        Value::UInt32(value) => value.to_string(),
        Value::UInt64(value) => value.to_string(),
        Value::Float32(value) => (*value as f64).to_string(),
        Value::Float(value) => value.to_string(),
        Value::BigInt(value) | Value::UInt128(value) => value.to_string(),
        Value::BigDecimal(value) => value.to_string(),
        Value::DateTime(value) | Value::String(value) => value.clone(),
        Value::InternalId { table, offset } => format!("{table}:{offset}"),
        Value::Node { label, id } => format!("{label}#{id}"),
        Value::Edge { rel_type, id, .. } => format!("{rel_type}#{id}"),
        Value::List(items) | Value::Path(items) => {
            let parts = items
                .iter()
                .map(display_for_list_to_string)
                .collect::<Vec<_>>();
            format!("[{}]", parts.join(","))
        }
        Value::Map(map) => {
            let parts = visible_map_keys(map)
                .into_iter()
                .filter_map(|key| {
                    map.get(&key)
                        .map(|value| format!("{key}: {}", display_for_list_to_string(value)))
                })
                .collect::<Vec<_>>();
            format!("{{{}}}", parts.join(", "))
        }
    }
}

/// Relationship-table index for `_ID` printing.
///
/// Kuzu numbers relationship tables in the same namespace as node tables and
/// reserves a second internal slot per relationship type for the reverse
/// adjacency table, so visible ids advance by two per type after the node
/// tables. This mirrors `interpreter::element_id::edge_table_index`; the two
/// must agree or the same edge prints with different ids depending on which
/// path ran it.
pub(super) fn rel_index_case(label_expr: Expr, graph: &PropertyGraph) -> Expr {
    let base = graph.node_label_order().len() as i64;
    label_index_case(label_expr, graph.edge_rel_order(), base, 2)
}

/// `CASE label WHEN l0 THEN '0' WHEN l1 THEN '1' ... END` — the catalog
/// insertion-order table index used by Kuzu-style `_ID` printing.
///
/// `base` and `stride` place the result in Kuzu's table-id numbering, which
/// node and relationship tables share. See [`rel_index_case`].
pub(super) fn label_index_case(label_expr: Expr, order: &[String], base: i64, stride: i64) -> Expr {
    let arms = order
        .iter()
        .enumerate()
        .map(|(idx, label)| {
            (
                Box::new(lit(label.clone())),
                Box::new(lit((base + idx as i64 * stride).to_string())),
            )
        })
        .collect::<Vec<_>>();
    if arms.is_empty() {
        return lit(base.to_string());
    }
    Expr::Case(Case::new(
        Some(Box::new(label_expr)),
        arms,
        Some(Box::new(lit("?"))),
    ))
}

#[cfg(test)]
mod folding_tests {
    use super::*;

    #[test]
    fn engine_calls_are_not_interpreter_constants_even_with_null_arguments() {
        for name in ["unknown_function", "stats", "uuid_extract_version"] {
            let call = IrExpr::Call {
                name: name.into(),
                args: vec![IrExpr::Lit(Lit::Null)],
            };
            assert!(!expr_is_constant(&call, &[]), "{name}");
            let nested = IrExpr::Call {
                name: "coalesce".into(),
                args: vec![call, IrExpr::Lit(Lit::Int(1))],
            };
            assert!(!expr_is_constant(&nested, &[]), "nested {name}");
        }
        assert!(constant_foldable_function("UPPER"));
        assert!(constant_foldable_function("tointeger"));
        assert!(!constant_foldable_function("random"));
    }
}
