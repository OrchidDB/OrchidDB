//! Union variant selection and type compatibility.

use crate::ir::runtime::{RuntimeError, IrResult};
use crate::ir::value::Value;
use std::collections::BTreeMap;
use super::{UNION_TAG_KEY, UNION_VALUE_KEY, UNION_VARIANTS_KEY};
use super::cast_conversion::{
    array_suffix, cast_value, mode_conversion_error, split_struct_field, type_argument,
};
use super::CastMode;
use super::cast_scalar::{UnionVariant, is_uuid_text};
use super::lists::split_top_level_commas;
use super::maps::make_union_value;

pub(super) fn cast_to_union_unified(
    value: &Value,
    fields: &str,
    target_type: &str,
    mode: CastMode,
) -> IrResult<Value> {
    let Some(variants) = parse_union_variants(fields) else {
        return mode_conversion_error(mode);
    };
    if variants.is_empty() {
        return mode_conversion_error(mode);
    }

    if let Value::Map(map) = value {
        if let Some((active_tag, active_value)) = union_payload(map) {
            if let Some(source_variants) = decode_union_variants(map) {
                for (source_tag, _) in &source_variants {
                    if !variants
                        .iter()
                        .any(|variant| variant.tag.eq_ignore_ascii_case(source_tag))
                    {
                        return Err(union_missing_field_error(
                            &source_variants,
                            target_type,
                            source_tag,
                        ));
                    }
                }
            }

            let Some(target) = variants
                .iter()
                .find(|variant| variant.tag.eq_ignore_ascii_case(active_tag))
            else {
                return mode_conversion_error(mode);
            };
            let casted = cast_union_payload(active_value, target.ty, true, mode)?;
            return Ok(make_union_value(&target.tag, casted, Some(&variants)));
        }
    }

    let Some(index) = select_union_variant(value, &variants, false) else {
        return mode_conversion_error(mode);
    };
    let variant = &variants[index];
    let casted = cast_value(value, variant.ty, CastMode::ExplicitStrict)?;
    Ok(make_union_value(&variant.tag, casted, Some(&variants)))
}

fn cast_union_payload(
    value: &Value,
    target_type: &str,
    allow_numeric_narrowing: bool,
    mode: CastMode,
) -> IrResult<Value> {
    if union_variant_score(value, target_type, allow_numeric_narrowing).is_none() {
        return mode_conversion_error(mode);
    }
    cast_value(value, target_type, CastMode::ExplicitStrict)
}

fn select_union_variant(
    value: &Value,
    variants: &[UnionVariant<'_>],
    allow_numeric_narrowing: bool,
) -> Option<usize> {
    variants
        .iter()
        .enumerate()
        .filter_map(|(index, variant)| {
            union_variant_score(value, variant.ty, allow_numeric_narrowing)
                .map(|score| (index, score))
        })
        .min_by_key(|(index, score)| (*score, *index))
        .map(|(index, _)| index)
}

fn union_variant_score(
    value: &Value,
    target_type: &str,
    allow_numeric_narrowing: bool,
) -> Option<u8> {
    let cleaned = target_type.trim();
    if matches!(value, Value::Null) {
        return Some(0);
    }
    if array_suffix(cleaned).is_some()
        || type_argument(cleaned, "LIST").is_some()
        || type_argument(cleaned, "ARRAY").is_some()
    {
        return matches!(value, Value::List(_) | Value::String(_))
            .then(|| {
                cast_value(value, cleaned, CastMode::ExplicitStrict)
                    .ok()
                    .map(|_| 2)
            })
            .flatten();
    }
    if type_argument(cleaned, "STRUCT").is_some() {
        return matches!(value, Value::Map(_) | Value::String(_))
            .then(|| {
                cast_value(value, cleaned, CastMode::ExplicitStrict)
                    .ok()
                    .map(|_| 2)
            })
            .flatten();
    }
    if type_argument(cleaned, "MAP").is_some() {
        return matches!(value, Value::Map(_) | Value::String(_))
            .then(|| {
                cast_value(value, cleaned, CastMode::ExplicitStrict)
                    .ok()
                    .map(|_| 2)
            })
            .flatten();
    }
    if type_argument(cleaned, "UNION").is_some() {
        return cast_value(value, cleaned, CastMode::ExplicitStrict)
            .ok()
            .map(|_| 2);
    }

    let head = type_head(cleaned);
    if is_string_target(&head) {
        return union_string_fallback_score(value);
    }
    if is_bool_target(&head) {
        return match value {
            Value::Bool(_) => Some(0),
            Value::String(_) => cast_value(value, cleaned, CastMode::ExplicitStrict)
                .ok()
                .map(|_| 4),
            _ => None,
        };
    }
    if is_numeric_target(&head) {
        return numeric_union_score(value, &head, cleaned, allow_numeric_narrowing);
    }
    if is_temporal_target(&head) {
        return temporal_union_score(value, &head, cleaned);
    }
    if head == "UUID" {
        return match value {
            Value::String(text) if is_uuid_text(text) => Some(0),
            Value::String(_) => cast_value(value, cleaned, CastMode::ExplicitStrict)
                .ok()
                .map(|_| 4),
            _ => None,
        };
    }
    if head == "INTERVAL" {
        return matches!(value, Value::String(_))
            .then(|| {
                cast_value(value, cleaned, CastMode::ExplicitStrict)
                    .ok()
                    .map(|_| 4)
            })
            .flatten();
    }

    cast_value(value, cleaned, CastMode::ExplicitStrict)
        .ok()
        .map(|_| 6)
}

fn union_string_fallback_score(value: &Value) -> Option<u8> {
    match value {
        Value::String(_) => Some(8),
        Value::Bool(_)
        | Value::Byte(_)
        | Value::UInt8(_)
        | Value::Short(_)
        | Value::UInt16(_)
        | Value::Int(_)
        | Value::UInt32(_)
        | Value::Long(_)
        | Value::UInt64(_)
        | Value::Float32(_)
        | Value::Float(_)
        | Value::BigInt(_)
        | Value::UInt128(_)
        | Value::BigDecimal(_)
        | Value::DateTime(_)
        | Value::InternalId { .. } => Some(8),
        _ => None,
    }
}

fn numeric_union_score(
    value: &Value,
    target_head: &str,
    target_type: &str,
    allow_numeric_narrowing: bool,
) -> Option<u8> {
    let source_rank = value_numeric_rank(value)?;
    let target_rank = numeric_target_rank(target_head)?;
    if numeric_exact_match(value, target_head) {
        return cast_value(value, target_type, CastMode::ExplicitStrict)
            .ok()
            .map(|_| 0);
    }
    if source_rank == 0 || allow_numeric_narrowing || target_rank >= source_rank {
        return cast_value(value, target_type, CastMode::ExplicitStrict)
            .ok()
            .map(|_| 2);
    }
    None
}

fn temporal_union_score(value: &Value, target_head: &str, target_type: &str) -> Option<u8> {
    match value {
        Value::DateTime(text) => datetime_union_score(text, target_head),
        Value::String(_) => cast_value(value, target_type, CastMode::ExplicitStrict)
            .ok()
            .map(|_| 4),
        _ => None,
    }
}

fn datetime_union_score(text: &str, target_head: &str) -> Option<u8> {
    let has_time = text.contains(' ') || text.contains('T');
    if !has_time {
        return (target_head == "DATE").then_some(0);
    }
    let has_zone = text.ends_with("+00") || text.ends_with('Z');
    if has_zone {
        return match target_head {
            "TIMESTAMP_TZ" => Some(0),
            "TIMESTAMP" | "DATETIME" => Some(2),
            _ => None,
        };
    }
    let fraction_digits = text
        .split_once('.')
        .map(|(_, fraction)| {
            fraction
                .chars()
                .take_while(|ch| ch.is_ascii_digit())
                .count()
        })
        .unwrap_or(0);
    match target_head {
        "TIMESTAMP_SEC" | "TIMESTAMP_S" if fraction_digits == 0 => Some(0),
        "TIMESTAMP_MS" if (1..=3).contains(&fraction_digits) => Some(0),
        "TIMESTAMP_NS" if fraction_digits > 3 => Some(0),
        "TIMESTAMP" | "DATETIME" => Some(2),
        "TIMESTAMP_MS" | "TIMESTAMP_NS" if fraction_digits == 0 => Some(3),
        "TIMESTAMP_NS" => Some(3),
        _ => None,
    }
}

fn parse_union_variants(fields: &str) -> Option<Vec<UnionVariant<'_>>> {
    split_top_level_commas(fields)
        .into_iter()
        .map(|field| {
            let trimmed = field.trim();
            if trimmed.is_empty() {
                return None;
            }
            if trimmed.contains(':') {
                return None;
            }
            if let Some((name, ty)) = split_struct_field(trimmed) {
                let tag = name.trim().trim_matches('"').trim_matches('\'').to_string();
                let ty = ty.trim();
                if tag.is_empty() || ty.is_empty() {
                    None
                } else {
                    Some(UnionVariant { tag, ty })
                }
            } else {
                Some(UnionVariant {
                    tag: type_head(trimmed).to_ascii_lowercase(),
                    ty: trimmed,
                })
            }
        })
        .collect()
}

pub(super) fn encode_union_variants(variants: &[UnionVariant<'_>]) -> Value {
    Value::List(
        variants
            .iter()
            .map(|variant| {
                Value::List(vec![
                    Value::String(variant.tag.clone()),
                    Value::String(variant.ty.to_string()),
                ])
            })
            .collect(),
    )
}

fn decode_union_variants(map: &BTreeMap<String, Value>) -> Option<Vec<(String, String)>> {
    let Value::List(items) = map.get(UNION_VARIANTS_KEY)? else {
        return None;
    };
    items
        .iter()
        .map(|item| {
            let Value::List(parts) = item else {
                return None;
            };
            let [Value::String(tag), Value::String(ty)] = parts.as_slice() else {
                return None;
            };
            Some((tag.clone(), ty.clone()))
        })
        .collect()
}

fn union_payload(map: &BTreeMap<String, Value>) -> Option<(&str, &Value)> {
    let Value::String(tag) = map.get(UNION_TAG_KEY)? else {
        return None;
    };
    let value = map.get(UNION_VALUE_KEY).or_else(|| map.get(tag))?;
    Some((tag.as_str(), value))
}

fn union_missing_field_error(
    source_variants: &[(String, String)],
    target_type: &str,
    missing_tag: &str,
) -> RuntimeError {
    RuntimeError::Runtime(format!(
        "Conversion exception: Cannot cast from {} to {}, target type is missing field '{}'.",
        union_type_display(source_variants),
        target_type,
        missing_tag
    ))
}

fn union_type_display(variants: &[(String, String)]) -> String {
    let fields = variants
        .iter()
        .map(|(tag, ty)| format!("{tag} {ty}"))
        .collect::<Vec<_>>()
        .join(", ");
    format!("UNION({fields})")
}

fn type_head(type_name: &str) -> String {
    let upper = type_name.trim().to_ascii_uppercase();
    let head = upper
        .split(|ch: char| ch.is_whitespace() || ch == '(' || ch == '[')
        .next()
        .unwrap_or("")
        .trim();
    match head {
        "INT" | "INTEGER" => "INT32".to_string(),
        "BIGINT" | "LONG" | "SERIAL" => "INT64".to_string(),
        "TINYINT" => "INT8".to_string(),
        "SMALLINT" => "INT16".to_string(),
        "REAL" | "FLOAT32" => "FLOAT".to_string(),
        "FLOAT64" => "DOUBLE".to_string(),
        "NUMERIC" => "DECIMAL".to_string(),
        "BOOLEAN" => "BOOL".to_string(),
        "TIMESTAMP_S" => "TIMESTAMP_SEC".to_string(),
        other => other.to_string(),
    }
}

fn is_string_target(head: &str) -> bool {
    matches!(
        head,
        "STRING" | "VARCHAR" | "CHAR" | "TEXT" | "BLOB" | "BYTEA"
    )
}

fn is_bool_target(head: &str) -> bool {
    head == "BOOL"
}

fn is_numeric_target(head: &str) -> bool {
    numeric_target_rank(head).is_some()
}

fn is_temporal_target(head: &str) -> bool {
    matches!(
        head,
        "DATE"
            | "TIMESTAMP"
            | "DATETIME"
            | "TIMESTAMP_NS"
            | "TIMESTAMP_MS"
            | "TIMESTAMP_SEC"
            | "TIMESTAMP_TZ"
    )
}

fn numeric_target_rank(head: &str) -> Option<u8> {
    Some(match head {
        "INT8" => 1,
        "UINT8" | "INT16" => 2,
        "UINT16" | "INT32" => 3,
        "UINT32" | "INT64" => 4,
        "UINT64" | "FLOAT" => 5,
        "INT128" | "UINT128" | "DOUBLE" | "DECIMAL" => 6,
        _ => return None,
    })
}

fn numeric_exact_match(value: &Value, target_head: &str) -> bool {
    matches!(
        (value, target_head),
        (Value::Byte(_), "INT8")
            | (Value::UInt8(_), "UINT8")
            | (Value::Short(_), "INT16")
            | (Value::UInt16(_), "UINT16")
            | (Value::Int(_), "INT64")
            | (Value::UInt32(_), "UINT32")
            | (Value::Long(_), "INT64")
            | (Value::UInt64(_), "UINT64")
            | (Value::BigInt(_), "INT128")
            | (Value::UInt128(_), "UINT128")
            | (Value::Float32(_), "FLOAT")
            | (Value::Float(_), "DOUBLE")
            | (Value::BigDecimal(_), "DECIMAL")
    )
}

fn value_numeric_rank(value: &Value) -> Option<u8> {
    Some(match value {
        Value::Byte(_) => 1,
        Value::UInt8(_) | Value::Short(_) => 2,
        Value::UInt16(_) => 3,
        Value::Int(_) | Value::UInt32(_) | Value::Long(_) => 4,
        Value::UInt64(_) => 5,
        Value::BigInt(_) | Value::UInt128(_) => 6,
        Value::Float32(_) => 5,
        Value::Float(_) | Value::BigDecimal(_) => 6,
        Value::String(_) => return Some(0),
        _ => return None,
    })
}
