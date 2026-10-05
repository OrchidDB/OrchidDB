//! Unicode operations use the same Rust case tables as the native evaluator.
//! SQL locale-dependent lower/upper/initcap are intentionally not substituted.
use std::sync::LazyLock;
struct CaseTables {
    mappings: String,
    properties: String,
}
static CASE: LazyLock<CaseTables> = LazyLock::new(|| {
    let mut mappings = Vec::new();
    let mut ranges = Vec::new();
    let mut run: Option<(u32, u32, u8)> = None;
    for n in 0..=0x10ffff {
        let Some(c) = char::from_u32(n) else {
            continue;
        };
        let lower = c.to_lowercase().collect::<String>();
        let upper = c.to_uppercase().collect::<String>();
        if lower != c.to_string() || upper != c.to_string() {
            mappings.push(format!("({n},'{lower}','{upper}')"));
        }
        // Derive the contextual final-sigma properties from Rust itself,
        // including characters that are both Cased and Case_Ignorable.
        let cased = format!("AΣ{c}").to_lowercase().chars().nth(1) == Some('σ');
        let ignored = !cased && format!("AΣ{c}A").to_lowercase().chars().nth(1) == Some('σ');
        let flags = u8::from(c.is_alphanumeric()) + 2 * u8::from(cased) + 4 * u8::from(ignored);
        if let Some((_, end, f)) = &mut run {
            if *f == flags && *end + 1 == n {
                *end = n;
                continue;
            }
        }
        if let Some((a, b, f)) = run.take() {
            if f != 0 {
                ranges.push(format!("({a},{b},{f})"));
            }
        }
        run = Some((n, n, flags));
    }
    if let Some((a, b, f)) = run {
        if f != 0 {
            ranges.push(format!("({a},{b},{f})"));
        }
    }
    CaseTables {
        mappings: mappings.join(","),
        properties: ranges.join(","),
    }
});
pub(super) fn mapping(name: &str, pg: bool) -> Option<String> {
    if name == "levenshtein" {
        return Some(distance(pg));
    }
    if !matches!(name, "upper" | "lower" | "initcap") {
        return None;
    }
    let code = if pg { "ascii" } else { "unicode" };
    let chars = if pg {
        "string_to_array(__local1, NULL)"
    } else {
        "string_split(__local1, '')"
    };
    let converted = match name {
        "upper" => "coalesce(__local13, __local3)",
        "lower" => {
            "CASE WHEN __local3 = 'Σ' AND (coalesce(__local26.__local8,0) & 2) = 2 AND (coalesce(__local27.__local8,0) & 2) = 0 THEN 'ς' ELSE coalesce(__local12, __local3) END"
        }
        _ => {
            "CASE WHEN coalesce(lag(__local8) OVER (ORDER BY __local4), 0) & 1 = 1 THEN coalesce(__local12, __local3) ELSE coalesce(__local13, __local3) END"
        }
    };
    let input = if name == "lower" {
        "(SELECT __local23.__local3, __local23.__local4, __local23.__local12, __local23.__local13, CASE WHEN __local23.__local3 = 'Σ' AND (coalesce(__local26.__local8,0) & 2) = 2 AND (coalesce(__local27.__local8,0) & 2) = 0 THEN 'ς' ELSE coalesce(__local23.__local12,__local23.__local3) END AS __local24 FROM (SELECT *, max(CASE WHEN (__local8 & 4)=0 THEN __local4 END) OVER (ORDER BY __local4 ROWS BETWEEN UNBOUNDED PRECEDING AND 1 PRECEDING) AS __local28, min(CASE WHEN (__local8 & 4)=0 THEN __local4 END) OVER (ORDER BY __local4 ROWS BETWEEN 1 FOLLOWING AND UNBOUNDED FOLLOWING) AS __local29 FROM __local22) AS __local23 LEFT JOIN __local22 AS __local26 ON __local26.__local4=__local23.__local28 LEFT JOIN __local22 AS __local27 ON __local27.__local4=__local23.__local29)".to_string()
    } else {
        format!("(SELECT __local4, {converted} AS __local24 FROM __local22 AS __local23)")
    };
    Some(super::unary(&format!(
        "(WITH __local20(__local10,__local12,__local13) AS (VALUES {}), __local21(__local5,__local6,__local7) AS (VALUES {}), __local22 AS (SELECT __local3,__local4,coalesce(__local7,0) AS __local8,__local12,__local13 FROM unnest({chars}) WITH ORDINALITY AS __local2(__local3,__local4) LEFT JOIN __local20 ON {code}(__local3)=__local10 LEFT JOIN __local21 ON {code}(__local3) BETWEEN __local5 AND __local6) SELECT CASE WHEN __local1 IS NULL THEN NULL ELSE coalesce(string_agg(__local24, '' ORDER BY __local4), '') END FROM {input} AS __local25)",
        CASE.mappings, CASE.properties
    )))
}
fn distance(pg: bool) -> String {
    let initial = if pg {
        "ARRAY(SELECT __local8 FROM generate_series(0::bigint, length(__local2)::bigint) AS __local9(__local8))"
    } else {
        "range(0, length(__local2)+1)"
    };
    // One dynamic-programming row, updated one cell per recursive step. SQL
    // substr counts code points; DuckDB's built-in distance counts bytes.
    super::bind(
        2,
        &format!(
            "(WITH RECURSIVE __local10(__local11,__local12,__local13) AS (SELECT 0::bigint, {initial}, 0::bigint UNION ALL SELECT __local11+1, ARRAY(SELECT CASE WHEN __local20=1 THEN __local16 + CASE WHEN __local15=length(__local2) THEN 1 ELSE 0 END WHEN __local20=__local15+1 THEN __local18 ELSE __local12[__local20] END FROM generate_series(1::bigint, length(__local2)::bigint+1) AS __local19(__local20) ORDER BY __local20), CASE WHEN __local15=length(__local2) THEN __local16 ELSE __local12[__local15+1] END FROM __local10 CROSS JOIN LATERAL (SELECT (__local11 % nullif(length(__local2),0))+1 AS __local15, CAST(floor(CAST(__local11 AS DECIMAL(38,0))/nullif(length(__local2),0)) AS BIGINT)+1 AS __local16) AS __local14 CROSS JOIN LATERAL (SELECT least(__local12[__local15+1]+1, CASE WHEN __local15=1 THEN __local16 ELSE __local12[__local15] END+1, __local13 + CASE WHEN substr(__local1,CAST(__local16 AS INTEGER),1)=substr(__local2,CAST(__local15 AS INTEGER),1) THEN 0 ELSE 1 END) AS __local18) AS __local17 WHERE __local11 < length(__local1)*length(__local2)) SELECT CASE WHEN __local1 IS NULL OR __local2 IS NULL THEN NULL WHEN length(__local2)=0 THEN length(__local1) ELSE __local12[length(__local2)+1] END FROM __local10 ORDER BY __local11 DESC LIMIT 1)"
        ),
    )
}

/// The pinned upstream overlay slices UTF-8 bytes at character offsets and
/// can panic. The portable wrapper defines character-based SQL overlay for
/// both native and pushed-down execution instead.
pub(crate) fn overlay(
    args: datafusion::logical_expr::ScalarFunctionArgs,
) -> datafusion::common::Result<datafusion::logical_expr::ColumnarValue> {
    use arrow::array::Array;
    use datafusion::{
        common::{DataFusionError, ScalarValue},
        logical_expr::ColumnarValue,
    };
    let scalar = args
        .args
        .iter()
        .all(|a| matches!(a, ColumnarValue::Scalar(_)));
    let arrays = ColumnarValue::values_to_arrays(&args.args)?;
    let mut out = Vec::with_capacity(arrays[0].len());
    for row in 0..arrays[0].len() {
        if arrays.iter().any(|a| a.is_null(row)) {
            out.push(ScalarValue::try_from(args.return_type())?);
            continue;
        }
        let string = |i: usize| -> datafusion::common::Result<String> {
            Ok(ScalarValue::try_from_array(arrays[i].as_ref(), row)?
                .cast_to(&arrow::datatypes::DataType::Utf8)?
                .to_string())
        };
        let number = |i: usize| -> datafusion::common::Result<i64> {
            match ScalarValue::try_from_array(arrays[i].as_ref(), row)?
                .cast_to(&arrow::datatypes::DataType::Int64)?
            {
                ScalarValue::Int64(Some(n)) => Ok(n),
                _ => unreachable!(),
            }
        };
        let s = string(0)?;
        let replacement = string(1)?;
        let start = number(2)?;
        let count = if arrays.len() == 4 {
            number(3)?
        } else {
            replacement.chars().count() as i64
        };
        if start <= 0 || count < 0 {
            return Err(DataFusionError::Execution(
                "overlay requires a positive start and nonnegative length".into(),
            ));
        }
        let from = usize::try_from(start - 1).unwrap_or(usize::MAX);
        let end = from.saturating_add(usize::try_from(count).unwrap_or(usize::MAX));
        let result = s
            .chars()
            .take(from)
            .chain(replacement.chars())
            .chain(s.chars().skip(end))
            .collect::<String>();
        out.push(ScalarValue::Utf8(Some(result)).cast_to(args.return_type())?);
    }
    if scalar {
        Ok(ColumnarValue::Scalar(out.remove(0)))
    } else if out.is_empty() {
        Ok(ColumnarValue::Array(arrow::array::new_empty_array(
            args.return_type(),
        )))
    } else {
        Ok(ColumnarValue::Array(ScalarValue::iter_to_array(out)?))
    }
}

pub(super) fn overload(name: &str, pg: bool, arity: usize) -> Option<String> {
    if name != "overlay" || !matches!(arity, 3 | 4) {
        return None;
    }
    let count = if arity == 4 {
        "__local4"
    } else {
        "length(__local2)"
    };
    let nulls = (1..=arity)
        .map(|i| format!("__local{i} IS NULL"))
        .collect::<Vec<_>>()
        .join(" OR ");
    let fail = if pg {
        "CAST(CAST('invalid overlay ' || __local3 AS BIGINT) AS TEXT)"
    } else {
        "error('invalid overlay position or length')"
    };
    Some(super::bind(
        arity,
        &format!(
            "CASE WHEN {nulls} THEN NULL WHEN __local3 <= 0 OR {count} < 0 THEN {fail} ELSE left(__local1, CAST(least(__local3 - 1, length(__local1)) AS INTEGER)) || __local2 || substr(__local1, CAST(least(CAST(__local3 AS DECIMAL(38,0)) + {count}, length(__local1) + 1) AS INTEGER)) END"
        ),
    ))
}
