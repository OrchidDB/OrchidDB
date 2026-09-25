//! Scalar function classification and cast type mapping.
use super::*;

pub(super) fn is_label_function(name: &str) -> bool {
    name.eq_ignore_ascii_case("label") || name.eq_ignore_ascii_case("cypher_label")
}

pub(super) fn is_id_function(name: &str) -> bool {
    name.eq_ignore_ascii_case("id") || name.eq_ignore_ascii_case("ID")
}

pub(super) fn is_mod_function(name: &str) -> bool {
    name.eq_ignore_ascii_case("mod")
}

pub(super) fn is_abs_function(name: &str) -> bool {
    name.eq_ignore_ascii_case("abs")
}

pub(super) fn is_pow_function(name: &str) -> bool {
    name.eq_ignore_ascii_case("pow") || name.eq_ignore_ascii_case("power")
}

pub(super) fn normalize_function_name(name: &str) -> String {
    name.to_ascii_lowercase().replace('-', "_")
}

pub(super) fn normalize_temporal_unit(unit: &str) -> String {
    let normalized = unit.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "years" => "year",
        "months" => "month",
        "weeks" => "week",
        "days" => "day",
        "hours" => "hour",
        "minutes" => "minute",
        "seconds" => "second",
        "milliseconds" => "millisecond",
        "microseconds" => "microsecond",
        "nanoseconds" => "nanosecond",
        "quarters" => "quarter",
        "decades" => "decade",
        "centuries" => "century",
        "millennia" | "millenniums" => "millennium",
        _ => normalized.as_str(),
    }
    .to_string()
}

pub(super) fn expression_has_wide_numeric_cast(expr: &IrExpr) -> bool {
    let IrExpr::Call { name, args } = expr else {
        return false;
    };
    cast_target_text(name, args).is_some_and(|target| {
        matches!(
            target
                .trim()
                .trim_matches('"')
                .to_ascii_uppercase()
                .as_str(),
            "INT128" | "UINT128"
        )
    })
}

pub(super) fn is_unary_math_function(name: &str) -> bool {
    matches!(
        normalize_function_name(name).as_str(),
        "acos"
            | "acosh"
            | "asin"
            | "asinh"
            | "atan"
            | "atanh"
            | "cbrt"
            | "ceil"
            | "ceiling"
            | "cos"
            | "cosh"
            | "cot"
            | "degrees"
            | "exp"
            | "factorial"
            | "floor"
            | "ln"
            | "log"
            | "log2"
            | "log10"
            | "radians"
            | "round"
            | "sign"
            | "signum"
            | "sin"
            | "sinh"
            | "sqrt"
            | "tan"
            | "tanh"
            | "trunc"
            | "truncate"
    )
}

pub(super) fn is_binary_math_function(name: &str) -> bool {
    matches!(
        normalize_function_name(name).as_str(),
        "atan2" | "gcd" | "lcm" | "log" | "nanvl" | "round" | "trunc" | "truncate"
    )
}

pub(super) fn is_date_function(name: &str) -> bool {
    name.eq_ignore_ascii_case("date") || name.eq_ignore_ascii_case("to_date")
}

pub(super) fn is_date_constructor(name: &str) -> bool {
    matches!(
        normalize_function_name(name).as_str(),
        "date" | "to_date" | "timestamp" | "interval" | "duration"
    )
}

pub(super) fn is_constant_collection_function(name: &str) -> bool {
    matches!(
        normalize_function_name(name).as_str(),
        "array_slice"
            | "array_append"
            | "array_cat"
            | "array_concat"
            | "array_contains"
            | "array_has"
            | "array_indexof"
            | "array_position"
            | "array_prepend"
            | "array_push_back"
            | "array_push_front"
            | "element_at"
            | "list_append"
            | "list_any_value"
            | "list_cat"
            | "list_concat"
            | "list_contains"
            | "list_distinct"
            | "list_element"
            | "list_extract"
            | "list_has_all"
            | "list_has"
            | "list_indexof"
            | "list_join"
            | "list_prepend"
            | "list_position"
            | "list_product"
            | "list_reverse"
            | "list_reverse_sort"
            | "list_slice"
            | "list_sort"
            | "list_sum"
            | "list_to_string"
            | "list_unique"
            | "map_keys"
    )
}

pub(super) fn is_string_function(name: &str) -> bool {
    matches!(
        normalize_function_name(name).as_str(),
        "char_length"
            | "character_length"
            | "concat"
            | "concat_ws"
            | "contains"
            | "ends_with"
            | "endswith"
            | "gremlin_lcase"
            | "gremlin_substring"
            | "gremlin_ucase"
            | "lcase"
            | "left"
            | "length"
            | "local_length"
            | "local_lcase"
            | "local_ltrim"
            | "local_reverse_strings"
            | "local_rtrim"
            | "local_trim"
            | "local_ucase"
            | "lower"
            | "lpad"
            | "ltrim"
            | "prefix"
            | "regexp_full_match"
            | "regexp_like"
            | "regexp_matches"
            | "regexp_replace"
            | "replace"
            | "reverse"
            | "right"
            | "rpad"
            | "rtrim"
            | "size"
            | "starts_with"
            | "startswith"
            | "strcontains"
            | "substr"
            | "substring"
            | "suffix"
            | "tolower"
            | "toupper"
            | "trim"
            | "ucase"
            | "upper"
    )
}

pub(super) fn is_core_variadic_function(name: &str) -> bool {
    matches!(
        normalize_function_name(name).as_str(),
        "coalesce" | "constant_or_null" | "greatest" | "ifnull" | "least" | "nullif"
    )
}

pub(super) fn is_exists_function(name: &str) -> bool {
    name.eq_ignore_ascii_case("exists")
}

pub(super) fn is_in_function(name: &str) -> bool {
    name.eq_ignore_ascii_case("in")
}

pub(super) fn is_cast_function(name: &str, args: &[IrExpr]) -> bool {
    let normalized = name.to_ascii_lowercase();
    (normalized == "cast" && args.len() == 2) || cast_target_from_function_name(&normalized).is_ok()
}

pub(super) fn cast_target_text<'a>(name: &'a str, args: &'a [IrExpr]) -> Option<&'a str> {
    let normalized = name.to_ascii_lowercase();
    if normalized == "cast" {
        let IrExpr::Lit(Lit::String(target)) = args.get(1)? else {
            return None;
        };
        Some(target)
    } else {
        cast_target_from_function_name(&normalized).ok()
    }
}

pub(super) fn cast_target_from_function_name(name: &str) -> RelResult<&'static str> {
    match name {
        "tointeger" => Ok("INT64"),
        "tofloat" => Ok("DOUBLE"),
        "toboolean" => Ok("BOOL"),
        "tostring" => Ok("STRING"),
        "to_bool" | "to_boolean" => Ok("BOOL"),
        "to_string" | "string" | "cast_string" => Ok("STRING"),
        "cast_byte" => Ok("INT8"),
        "cast_short" => Ok("INT16"),
        "to_int8" => Ok("INT8"),
        "to_int16" => Ok("INT16"),
        "to_int32" | "cast_int" | "gremlin_cast_int" => Ok("INT32"),
        "to_int64" | "to_serial" | "cast_long" => Ok("INT64"),
        "to_int128" => Ok("INT128"),
        "to_uint8" => Ok("UINT8"),
        "to_uint16" => Ok("UINT16"),
        "to_uint32" => Ok("UINT32"),
        "to_uint64" => Ok("UINT64"),
        "to_uint128" => Ok("UINT128"),
        "to_float" | "cast_float" => Ok("FLOAT"),
        "to_double" | "cast_double" => Ok("DOUBLE"),
        "cast_bool" | "cast_boolean" => Ok("BOOL"),
        "cast_bigint" => Ok("DECIMAL(38,0)"),
        "cast_bigdecimal" => Ok("DECIMAL(38,6)"),
        _ => Err(RelError::Unsupported(format!(
            "function `{name}` is not relationally lowered yet"
        ))),
    }
}

pub(super) fn data_type_for_cast_target(type_name: &str) -> RelResult<DataType> {
    let normalized = type_name
        .trim()
        .trim_matches('"')
        .to_ascii_uppercase()
        .replace(' ', "");
    if let Some(decimal) = normalized
        .strip_prefix("DECIMAL(")
        .and_then(|value| value.strip_suffix(')'))
    {
        let mut parts = decimal.split(',');
        let precision = parts
            .next()
            .and_then(|value| value.parse::<u8>().ok())
            .ok_or_else(|| {
                RelError::Unsupported(format!("invalid decimal target `{type_name}`"))
            })?;
        let scale = parts
            .next()
            .and_then(|value| value.parse::<i8>().ok())
            .ok_or_else(|| {
                RelError::Unsupported(format!("invalid decimal target `{type_name}`"))
            })?;
        if parts.next().is_some() {
            return Err(RelError::Unsupported(format!(
                "invalid decimal target `{type_name}`"
            )));
        }
        return Ok(DataType::Decimal128(precision, scale));
    }
    match normalized.as_str() {
        "BOOL" | "BOOLEAN" => Ok(DataType::Boolean),
        "INT8" => Ok(DataType::Int8),
        "INT16" => Ok(DataType::Int16),
        "INT32" => Ok(DataType::Int32),
        "INT64" | "SERIAL" => Ok(DataType::Int64),
        "UINT8" => Ok(DataType::UInt8),
        "UINT16" => Ok(DataType::UInt16),
        "UINT32" => Ok(DataType::UInt32),
        "UINT64" => Ok(DataType::UInt64),
        "FLOAT" => Ok(DataType::Float32),
        "DOUBLE" | "FLOAT64" => Ok(DataType::Float64),
        "DECIMAL" => Ok(DataType::Decimal128(18, 3)),
        "STRING" | "VARCHAR" | "UUID" => Ok(DataType::Utf8),
        "DATE" => Ok(DataType::Date32),
        "TIMESTAMP" | "TIMESTAMP_US" => Ok(DataType::Timestamp(
            arrow::datatypes::TimeUnit::Microsecond,
            None,
        )),
        "TIMESTAMP_NS" => Ok(DataType::Timestamp(
            arrow::datatypes::TimeUnit::Nanosecond,
            None,
        )),
        "TIMESTAMP_MS" => Ok(DataType::Timestamp(
            arrow::datatypes::TimeUnit::Millisecond,
            None,
        )),
        "TIMESTAMP_SEC" => Ok(DataType::Timestamp(
            arrow::datatypes::TimeUnit::Second,
            None,
        )),
        "TIMESTAMP_TZ" => Ok(DataType::Timestamp(
            arrow::datatypes::TimeUnit::Microsecond,
            Some("UTC".into()),
        )),
        // 128-bit integers have no native Arrow representation; a
        // zero-scale decimal covers the numeric range these cases use and
        // prints identically. Values beyond 38 digits fail the cast, which
        // surfaces as an execution error rather than a wrong result.
        "INT128" | "UINT128" => Ok(DataType::Decimal128(38, 0)),
        other => Err(RelError::Unsupported(format!(
            "cast target `{other}` is not relationally lowered yet"
        ))),
    }
}

pub(super) fn normalize_type_name(type_name: &str) -> String {
    type_name
        .trim()
        .trim_start_matches("GType.")
        .trim_start_matches("java.lang.")
        .trim_start_matches("java.math.")
        .to_ascii_lowercase()
}

pub(super) fn data_type_matches_gremlin_type(data_type: &DataType, type_name: &str) -> bool {
    match data_type {
        DataType::Null => type_name == "null",
        DataType::Boolean => matches!(type_name, "boolean" | "bool"),
        DataType::Int8 => type_name == "byte",
        DataType::UInt8 => matches!(type_name, "uint8" | "byte"),
        DataType::Int16 => type_name == "short",
        DataType::UInt16 => type_name == "uint16",
        DataType::Int32 => matches!(type_name, "int" | "integer"),
        DataType::UInt32 => type_name == "uint32",
        DataType::Int64 => matches!(type_name, "long" | "int" | "integer"),
        DataType::UInt64 => type_name == "uint64",
        DataType::Float32 => type_name == "float",
        DataType::Float64 => type_name == "double",
        DataType::Utf8 | DataType::LargeUtf8 | DataType::Utf8View => {
            matches!(type_name, "string" | "char" | "character")
        }
        DataType::List(_) | DataType::LargeList(_) | DataType::FixedSizeList(_, _) => {
            matches!(type_name, "list" | "set" | "graph")
        }
        _ => false,
    }
}
