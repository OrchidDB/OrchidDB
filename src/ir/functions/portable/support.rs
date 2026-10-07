//! Typed eligibility is shared by mixed execution and SQL-only compilation.
use arrow::datatypes::DataType;
use datafusion::{
    common::{DFSchema, ScalarValue},
    logical_expr::{Expr, ExprSchemable},
};

/// A mapping may cover only part of a native function's signature. Reject that
/// call before changing its Arrow representation or selecting a SQL island.
pub fn validate_call(
    name: &str,
    args: &[Expr],
    schema: &DFSchema,
    dialect: &str,
) -> Result<(), &'static str> {
    let name = name.strip_prefix("fn.").unwrap_or(name);
    let types = args
        .iter()
        .map(|a| a.get_type(schema))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| "cannot determine portable argument types")?;
    if dialect == "postgres"
        && matches!(name, "greatest" | "least")
        && types
            .iter()
            .any(|t| matches!(t, DataType::Utf8 | DataType::Utf8View | DataType::LargeUtf8))
    {
        return Err("PostgreSQL text greatest/least ordering depends on collation");
    }
    if super::structured::handles(name) {
        super::structured::mapping(name, args, schema, dialect == "postgres").map_err(
            |_| "structured mapping requires valid typed fields and literal field names",
        )?;
    }
    let integer_literal = |i: usize| args.get(i).and_then(literal_integer);
    if dialect == "postgres"
        && name == "chr"
        && !integer_literal(0).is_some_and(|v| (v as u32) != 0)
    {
        return Err(
            "PostgreSQL chr requires a literal that does not wrap to NUL, which PostgreSQL text cannot represent",
        );
    }
    if dialect == "postgres"
        && name == "to_timestamp_nanos"
        && !integer_literal(0).is_some_and(|v| v % 1000 == 0)
    {
        return Err("PostgreSQL nanosecond epoch requires a microsecond-aligned literal");
    }
    if name.starts_with("regexp_") {
        let Some(Expr::Literal(
            ScalarValue::Utf8(Some(pattern))
            | ScalarValue::Utf8View(Some(pattern))
            | ScalarValue::LargeUtf8(Some(pattern)),
            _,
        )) = args.get(1)
        else {
            return Err(
                "regex SQL mapping requires a literal pattern in the common Rust/RE2/POSIX subset",
            );
        };
        if !pattern.is_ascii()
            || pattern
                .chars()
                .any(|c| !c.is_ascii_alphanumeric() && !" ._-+*?^$[]".contains(c))
            || ["[^", "*?", "+?", "??"].iter().any(|s| pattern.contains(s))
            || ::regex::Regex::new(pattern).map_or(true, |r| r.is_match(""))
        {
            return Err(
                "regex SQL mapping excludes captures, alternation, Unicode classes, lazy quantifiers, and empty matches",
            );
        }
        if name == "regexp_replace" {
            if !matches!(args.get(2), Some(Expr::Literal(ScalarValue::Utf8(Some(s)) | ScalarValue::Utf8View(Some(s)) | ScalarValue::LargeUtf8(Some(s)), _)) if !s.contains(['\\', '$']))
            {
                return Err(
                    "regex replacement SQL mapping requires a literal replacement without backreferences",
                );
            }
        }
    }
    if matches!(name, "range" | "generate_series") {
        if types.iter().any(|t| *t != DataType::Int64)
            || args.len() == 3 && !integer_literal(2).is_some_and(|n| n != 0)
        {
            return Err("series SQL mapping requires Int64 bounds and a nonzero literal step");
        }
    }
    if name.starts_with("array_")
        || matches!(name, "cardinality" | "empty" | "flatten" | "make_array")
    {
        // PostgreSQL's ragged/nested arrays use a JSON representation, whereas
        // these implementations operate on actual one-dimensional SQL arrays.
        for (index, ty) in types.iter().enumerate() {
            if name == "array_distance" {
                let source = match &args[index] {
                    Expr::Cast(c) => c.expr.get_type(schema).ok(),
                    _ => Some(ty.clone()),
                };
                if matches!(source, Some(DataType::List(f) | DataType::LargeList(f)) if matches!(f.data_type(), DataType::Int8 | DataType::Int16 | DataType::Int32 | DataType::Int64))
                {
                    continue;
                }
                return Err(
                    "array_distance SQL mapping requires signed-integer source elements before Float64 coercion",
                );
            }
            let inner = match ty {
                DataType::List(f) | DataType::LargeList(f) => f.data_type(),
                other => other,
            };
            if !matches!(
                inner,
                DataType::Boolean
                    | DataType::Int8
                    | DataType::Int16
                    | DataType::Int32
                    | DataType::Int64
                    | DataType::Utf8
                    | DataType::LargeUtf8
                    | DataType::Utf8View
                    | DataType::Null
            ) {
                return Err(
                    "array mapping requires flat boolean, signed integer, or text elements",
                );
            }
        }
        if matches!(name, "array_min" | "array_max" | "array_sort")
            && dialect == "postgres"
            && matches!(types.first(), Some(DataType::List(f) | DataType::LargeList(f)) if matches!(f.data_type(), DataType::Utf8 | DataType::Utf8View | DataType::LargeUtf8))
        {
            return Err("PostgreSQL text array ordering depends on collation");
        }
        if name == "array_position" && args.len() == 3 && !integer_literal(2).is_some_and(|v| v > 0)
        {
            return Err("array_position start must be a positive literal");
        }
        if name == "array_resize" && !integer_literal(1).is_some_and(|v| v >= 0) {
            return Err("array_resize size must be a nonnegative literal");
        }
        if name == "array_slice" && args.len() == 4 && !integer_literal(3).is_some_and(|v| v > 0) {
            return Err("array_slice stride SQL mapping requires a positive literal");
        }
        if name == "array_sort" {
            let text = |i| matches!(args.get(i), Some(Expr::Literal(ScalarValue::Utf8(Some(v)) | ScalarValue::Utf8View(Some(v)) | ScalarValue::LargeUtf8(Some(v)), _)) if if i == 1 { matches!(v.as_str(), "ASC" | "DESC") } else { matches!(v.as_str(), "NULLS FIRST" | "NULLS LAST") });
            if args.len() > 1 && !text(1) || args.len() > 2 && !text(2) {
                return Err(
                    "array_sort SQL mapping requires literal ASC/DESC and NULLS FIRST/LAST modifiers",
                );
            }
        }
        if name == "make_array"
            && (args.is_empty()
                || types
                    .iter()
                    .any(|t| matches!(t, DataType::List(_) | DataType::LargeList(_))))
        {
            return Err("make_array SQL mapping requires at least one scalar element");
        }
        if name == "array_repeat" && !integer_literal(1).is_some_and(|n| n >= 0) {
            return Err("array_repeat SQL mapping requires a nonnegative literal count");
        }
        if name == "array_remove_n" && integer_literal(2).is_none()
            || name == "array_replace_n" && integer_literal(3).is_none()
        {
            return Err("array remove/replace count must be a non-null integer literal");
        }
        if name == "array_to_string"
            && !matches!(types.first(), Some(DataType::List(f) | DataType::LargeList(f)) if matches!(f.data_type(), DataType::Utf8 | DataType::LargeUtf8 | DataType::Utf8View))
        {
            return Err(
                "array_to_string SQL mapping requires text elements; scalar formatting differs",
            );
        }
    }
    if matches!(
        name,
        "left" | "right" | "repeat" | "lpad" | "rpad" | "substr"
    ) {
        if !integer_literal(1).is_some_and(|v| i32::try_from(v).is_ok()) {
            return Err("text position/count SQL mapping requires an Int32-range literal");
        }
        if name == "substr"
            && args.len() == 3
            && !integer_literal(2).is_some_and(|v| v >= 0 && i32::try_from(v).is_ok())
        {
            return Err("substring length SQL mapping requires a nonnegative Int32-range literal");
        }
    }
    if name == "split_part"
        && !integer_literal(2).is_some_and(|v| v != 0 && i32::try_from(v).is_ok())
    {
        return Err("split_part SQL mapping requires a nonzero Int32-range literal index");
    }
    if matches!(name, "round" | "trunc")
        && (types.first() != Some(&DataType::Float64)
            || (args.len() == 2 && !integer_literal(1).is_some_and(|v| (-15..=15).contains(&v))))
    {
        return Err(
            "round/trunc SQL mapping requires Float64 and literal precision between -15 and 15",
        );
    }
    if matches!(name, "exp" | "sinh" | "cosh" | "cot") && types.first() != Some(&DataType::Float64)
    {
        return Err("transcendental SQL mapping requires Float64 to preserve overflow behavior");
    }
    if name.starts_with("to_timestamp") || name == "from_unixtime" {
        if args.len() != 1 || !matches!(types.first(), Some(DataType::Int64)) {
            return Err(
                "epoch SQL mapping accepts one Int64 argument; text parsing and timezone overloads require native execution",
            );
        }
    }
    let text_literal = |i: usize| -> Option<&str> {
        match args.get(i)? {
            Expr::Literal(
                ScalarValue::Utf8(Some(v))
                | ScalarValue::Utf8View(Some(v))
                | ScalarValue::LargeUtf8(Some(v)),
                _,
            ) => Some(v),
            _ => None,
        }
    };
    if matches!(name, "date_part" | "date_trunc") {
        if !text_literal(0).is_some_and(|s| {
            matches!(
                s,
                "year" | "quarter" | "month" | "week" | "day" | "hour" | "minute" | "second"
            ) || name == "date_part" && matches!(s, "doy" | "dow" | "isodow" | "isoyear")
        }) {
            return Err("date SQL mapping requires a supported literal unit");
        }
        if !matches!(
            types.get(1),
            Some(DataType::Timestamp(
                arrow::datatypes::TimeUnit::Second
                    | arrow::datatypes::TimeUnit::Millisecond
                    | arrow::datatypes::TimeUnit::Microsecond,
                None
            ))
        ) {
            return Err(
                "date SQL mapping requires a timezone-free timestamp of at most microsecond precision",
            );
        }
    }
    if matches!(name, "to_date" | "to_local_time")
        && (args.len() != 1
            || !matches!(
                types.first(),
                Some(DataType::Date32 | DataType::Timestamp(_, None))
            ))
    {
        return Err("temporal cast SQL mapping requires a single timezone-free date/timestamp");
    }
    if name == "make_time"
        && (!integer_literal(1).is_some_and(|v| (0..60).contains(&v))
            || !integer_literal(2).is_some_and(|v| (0..60).contains(&v)))
    {
        return Err("make_time SQL mapping requires literal minute and second fields in 0..59");
    }
    if matches!(name, "encode" | "decode")
        && !text_literal(1).is_some_and(|s| matches!(s, "hex" | "base64" | "base64pad"))
    {
        return Err("encoding SQL mapping supports literal hex, base64, and base64pad formats");
    }
    if name == "to_unixtime"
        && (args.len() != 1
            || !matches!(
                types.first(),
                Some(DataType::Timestamp(
                    arrow::datatypes::TimeUnit::Second
                        | arrow::datatypes::TimeUnit::Millisecond
                        | arrow::datatypes::TimeUnit::Microsecond,
                    None
                ))
            ))
    {
        return Err(
            "to_unixtime SQL mapping requires a timezone-free timestamp of at most microsecond precision",
        );
    }
    if name == "to_time"
        && (args.len() != 1
            || !matches!(
                types.first(),
                Some(
                    DataType::Time32(_) | DataType::Time64(arrow::datatypes::TimeUnit::Microsecond)
                )
            ))
    {
        return Err("to_time SQL mapping requires a time value of at most microsecond precision");
    }
    if name == "to_char"
        && (!matches!(
            text_literal(1),
            Some("%Y-%m-%d" | "%H:%M:%S" | "%Y-%m-%d %H:%M:%S")
        ) || !matches!(
            types.first(),
            Some(
                DataType::Date32
                    | DataType::Timestamp(
                        arrow::datatypes::TimeUnit::Second
                            | arrow::datatypes::TimeUnit::Millisecond
                            | arrow::datatypes::TimeUnit::Microsecond,
                        None
                    )
            )
        ))
    {
        return Err(
            "to_char SQL mapping supports ISO date/time formats on timezone-free dates/timestamps",
        );
    }
    if name == "power" && dialect == "postgres" {
        if !matches!(args.get(1), Some(Expr::Literal(ScalarValue::Float64(Some(v)), _)) if [0.0, 0.5, 1.0, 2.0].contains(v))
        {
            return Err(
                "PostgreSQL power maps literal exponents 0, 0.5, 1, and 2 exactly; other powers retain native IEEE behavior",
            );
        }
    }
    if name == "power" && types.iter().any(|t| *t != DataType::Float64) {
        return Err(
            "power SQL mapping requires Float64; integer and decimal power have different overflow semantics",
        );
    }
    if name == "digest"
        && !text_literal(1)
            .is_some_and(|s| matches!(s, "md5" | "sha224" | "sha256" | "sha384" | "sha512"))
    {
        return Err("digest SQL mapping requires a literal algorithm available without extensions");
    }
    if name == "date_bin" {
        let microsecond_source = |expr: &Expr| {
            let expr = if let Expr::Cast(c) = expr {
                c.expr.as_ref()
            } else {
                expr
            };
            matches!(
                expr.get_type(schema),
                Ok(DataType::Timestamp(
                    arrow::datatypes::TimeUnit::Microsecond,
                    None
                ))
            ) || matches!(expr, Expr::Literal(ScalarValue::TimestampNanosecond(Some(v), None), _) if v % 1000 == 0)
        };
        if !matches!(args.first(), Some(Expr::Literal(ScalarValue::IntervalMonthDayNano(Some(v)), _)) if v.months == 0 && v.days >= 0 && v.nanoseconds >= 0 && (v.days > 0 || v.nanoseconds > 0) && v.nanoseconds % 1000 == 0)
            || !args.get(1).is_some_and(microsecond_source)
            || !args.get(2).is_some_and(microsecond_source)
        {
            return Err(
                "date_bin SQL mapping requires a positive fixed microsecond stride and timezone-free microsecond source/origin",
            );
        }
    }
    Ok(())
}

fn literal_integer(expr: &Expr) -> Option<i64> {
    match expr {
        Expr::Literal(v, _) if v.data_type().is_integer() => {
            match v.cast_to(&DataType::Int64).ok()? {
                ScalarValue::Int64(n) => n,
                _ => None,
            }
        }
        Expr::Cast(cast) => literal_integer(&cast.expr),
        _ => None,
    }
}

/// Preparation can replace a stable function with a typed literal. Precision
/// checks must survive that rewrite, even though the UDF identity is gone.
pub fn literal_issue(value: &ScalarValue, dialect: &str) -> Option<&'static str> {
    if dialect == "postgres"
        && matches!(value,
        ScalarValue::TimestampNanosecond(Some(v), _) | ScalarValue::Time64Nanosecond(Some(v)) if v % 1000 != 0)
    {
        Some("PostgreSQL cannot preserve sub-microsecond timestamp/time precision")
    } else {
        None
    }
}
