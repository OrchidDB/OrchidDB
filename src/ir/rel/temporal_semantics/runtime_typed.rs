//! Runtime SQL operations on schema-declared temporal kinds. Encoded storage is
//! decoded into numeric calendar/clock components; it is never ordered as text.
use super::*;
use crate::ir::functions::{
    domain,
    logical::{LogicalFunction, SqlFunctionMapping},
};
use crate::ir::temporal::{TemporalValue, transport};
use chrono::{Datelike, Timelike};
use datafusion::logical_expr::{ColumnarValue, Volatility, create_udf};

fn function(
    name: &str,
    args: Vec<Expr>,
    inputs: Vec<DataType>,
    output: DataType,
    sql: String,
    evaluate: impl Fn(&[ScalarValue]) -> datafusion::common::Result<ScalarValue> + Send + Sync + 'static,
) -> Expr {
    use std::hash::{Hash, Hasher};
    let mut identity = std::collections::hash_map::DefaultHasher::new();
    sql.hash(&mut identity);
    inputs.hash(&mut identity);
    output.hash(&mut identity);
    let name = format!("{name}_{:x}", identity.finish());
    let result_type = output.clone();
    let native = create_udf(
        &name,
        inputs,
        output,
        Volatility::Immutable,
        Arc::new(move |args| {
            let length = args.iter().find_map(|a| {
                if let ColumnarValue::Array(a) = a {
                    Some(a.len())
                } else {
                    None
                }
            });
            let row = |index| {
                args.iter()
                    .map(|a| match a {
                        ColumnarValue::Scalar(s) => Ok(s.clone()),
                        ColumnarValue::Array(a) => ScalarValue::try_from_array(a, index),
                    })
                    .collect::<datafusion::common::Result<Vec<_>>>()
            };
            if let Some(length) = length {
                if length == 0 {
                    return Ok(ColumnarValue::Array(arrow::array::new_empty_array(
                        &result_type,
                    )));
                }
                let values = (0..length)
                    .map(|i| evaluate(&row(i)?))
                    .collect::<datafusion::common::Result<Vec<_>>>()?;
                ScalarValue::iter_to_array(values).map(ColumnarValue::Array)
            } else {
                evaluate(&row(0)?).map(ColumnarValue::Scalar)
            }
        }),
    );
    LogicalFunction::new(
        &name,
        Arc::new(native),
        BTreeMap::from([(
            "duckdb".into(),
            SqlFunctionMapping {
                value: sql,
                ordering: None,
            },
        )]),
    )
    .into_udf()
    .call(args)
}

fn kind(ty: &DataType) -> Option<&str> {
    if transport::is_date_type(ty) {
        Some("date")
    } else {
        transport::encoded_kind(ty)
    }
}
fn operand(context: &LoweringContext<'_>, plan: &LogicalPlan, expr: &IrExpr) -> Option<Expr> {
    // lower_expr accepts typed leaves without reentering this operation.
    context.lower_expr(plan, expr).ok()
}
fn payload() -> &'static str {
    "split_part(__arg0, '|', 2)"
}
fn clock(k: &str) -> String {
    if matches!(k, "datetime" | "localdatetime") {
        format!("split_part({}, 'T', 2)", payload())
    } else {
        payload().into()
    }
}
fn field(text: &str, pattern: &str, group: usize) -> String {
    format!(
        "CASE WHEN {text} IS NULL THEN NULL ELSE CAST(COALESCE(NULLIF(regexp_extract({text}, '{pattern}', {group}), ''), '0') AS BIGINT) END"
    )
}
fn seconds(k: &str) -> String {
    let t = clock(k);
    format!(
        "({} * 3600 + {} * 60 + {})",
        field(&t, r"^(\d{2}):(\d{2})(:(\d{2}))?", 1),
        field(&t, r"^(\d{2}):(\d{2})(:(\d{2}))?", 2),
        field(&t, r"^(\d{2}):(\d{2})(:(\d{2}))?", 4)
    )
}
fn nanos(k: &str) -> String {
    format!(
        "CAST(rpad(regexp_extract({}, '\\.(\\d+)', 1), 9, '0') AS BIGINT)",
        clock(k)
    )
}
fn offset_text(k: &str) -> String {
    format!(
        "regexp_extract({}, '(Z|[+-][0-9]{{2}}:[0-9]{{2}}(:[0-9]{{2}})?)(\\[|$)', 1)",
        clock(k)
    )
}
fn offset(k: &str) -> String {
    let text = offset_text(k);
    format!(
        "((CASE WHEN left({text}, 1) = '-' THEN -1 ELSE 1 END) * ({} * 3600 + {} * 60 + {}))",
        field(&text, r"^[+-](\d{2}):(\d{2})(:(\d{2}))?", 1),
        field(&text, r"^[+-](\d{2}):(\d{2})(:(\d{2}))?", 2),
        field(&text, r"^[+-](\d{2}):(\d{2})(:(\d{2}))?", 4)
    )
}
fn date_field(index: usize) -> String {
    field(payload(), r"^([+-]?\d+)-(\d{2})-(\d{2})", index)
}
fn epoch_seconds() -> String {
    // Strip fractions before the TIMESTAMP cast; nanoseconds remain a separate
    // integer. Epoch seconds are integral and exact throughout chrono's range.
    format!(
        "(CAST(epoch(CAST(regexp_extract({}, '^([+-]?[0-9]+-[0-9]{{2}}-[0-9]{{2}}T[0-9]{{2}}:[0-9]{{2}}(:[0-9]{{2}})?)', 1) AS TIMESTAMP)) AS BIGINT) - {})",
        payload(),
        offset("datetime")
    )
}
fn null_guard(sql: String) -> String {
    format!("CASE WHEN __arg0 IS NULL THEN NULL ELSE {sql} END")
}
fn bind_operands(sql: String, count: usize) -> String {
    let mut body = sql;
    let fields = (0..count)
        .map(|i| format!("__arg{i} AS __local{}", 80 + i))
        .collect::<Vec<_>>()
        .join(", ");
    for i in 0..count {
        body = body.replace(&format!("__arg{i}"), &format!("__local{}", 80 + i));
    }
    format!("(SELECT {body} FROM (SELECT {fields}) AS __local89)")
}

pub(in crate::ir::rel) fn order_keys(
    context: &LoweringContext<'_>,
    plan: &LogicalPlan,
    expr: &IrExpr,
) -> Option<Vec<Expr>> {
    let expr = match expr {
        IrExpr::Call { name, args } if name == "cypher_order_key" && args.len() == 1 => &args[0],
        other => other,
    };
    let value = operand(context, plan, expr)?;
    let ty = value.get_type(plan.schema()).ok()?;
    let k = kind(&ty)?;
    if transport::is_date_type(&ty) {
        return Some(vec![value]);
    }
    let components = match k {
        "date" => vec![date_field(1), date_field(2), date_field(3)],
        "localdatetime" => vec![
            date_field(1),
            date_field(2),
            date_field(3),
            seconds(k),
            nanos(k),
        ],
        "localtime" => vec![seconds(k), nanos(k)],
        "time" => vec![
            format!("({} - {})", seconds(k), offset(k)),
            nanos(k),
            offset(k),
        ],
        "datetime" => vec![epoch_seconds(), nanos(k), offset(k)],
        _ => return None,
    };
    let output = DataType::List(Arc::new(arrow::datatypes::Field::new(
        "item",
        DataType::Int64,
        true,
    )));
    let out = output.clone();
    let sql = bind_operands(
        null_guard(format!("list_value({})", components.join(", "))),
        1,
    );
    let key = function(
        "cypher_temporal_order_components",
        vec![value.clone()],
        vec![ty.clone()],
        output,
        sql,
        move |args| {
            if args[0].is_null() {
                return ScalarValue::try_from(&out);
            }
            let v = transport::decode(&args[0])?;
            let nums: Vec<i64> = match &v {
                TemporalValue::Date(_) | TemporalValue::WideDate(_) => {
                    let d = v.calendar_date().unwrap();
                    vec![d.year.into(), d.month.into(), d.day.into()]
                }
                TemporalValue::LocalDateTime(_) | TemporalValue::WideLocalDateTime(..) => {
                    let d = v.calendar_date().unwrap();
                    let t = v.time().unwrap();
                    vec![
                        d.year.into(),
                        d.month.into(),
                        d.day.into(),
                        t.num_seconds_from_midnight().into(),
                        t.nanosecond().into(),
                    ]
                }
                TemporalValue::LocalTime(t) => {
                    vec![t.num_seconds_from_midnight().into(), t.nanosecond().into()]
                }
                TemporalValue::Time(t, o) => vec![
                    i64::from(t.num_seconds_from_midnight()) - i64::from(*o),
                    t.nanosecond().into(),
                    (*o).into(),
                ],
                TemporalValue::DateTime(d, _) => vec![
                    d.timestamp(),
                    d.nanosecond().into(),
                    d.offset().local_minus_utc().into(),
                ],
                _ => {
                    return Err(DataFusionError::Execution(
                        "Temporal value has no ordering".into(),
                    ));
                }
            };
            Ok(ScalarValue::List(ScalarValue::new_list_nullable(
                &nums
                    .into_iter()
                    .map(|n| ScalarValue::Int64(Some(n)))
                    .collect::<Vec<_>>(),
                &DataType::Int64,
            )))
        },
    );
    let mut keys = vec![key];
    if k == "datetime" {
        keys.push(function(
            "cypher_temporal_order_zone",
            vec![value],
            vec![ty],
            DataType::Utf8,
            bind_operands(
                null_guard(format!(
                    "regexp_extract({}, '\\[([^]]+)\\]$', 1)",
                    payload()
                )),
                1,
            ),
            |args| {
                if args[0].is_null() {
                    return Ok(ScalarValue::Utf8(None));
                }
                let TemporalValue::DateTime(_, zone) = transport::decode(&args[0])? else {
                    return Err(DataFusionError::Execution("Expected datetime".into()));
                };
                Ok(ScalarValue::Utf8(Some(zone.unwrap_or_default())))
            },
        ));
    }
    Some(keys)
}

fn component_sql(k: &str, native_date: bool, name: &str) -> Option<String> {
    let key = name.to_ascii_lowercase();
    if k == "duration" {
        let m = "CAST(split_part(__arg0, '|', 2) AS BIGINT)";
        let d = "CAST(split_part(__arg0, '|', 3) AS BIGINT)";
        let s = "CAST(split_part(__arg0, '|', 4) AS BIGINT)";
        let n = "CAST(split_part(__arg0, '|', 5) AS BIGINT)";
        return Some(match key.as_str() {
            "years" => format!("({m} // 12)"),
            "quarters" => format!("({m} // 3)"),
            "months" => m.into(),
            "monthsofyear" => format!("({m} % 12)"),
            "monthsofquarter" => format!("({m} % 3)"),
            "quartersofyear" => format!("({m} // 3 % 4)"),
            "weeks" => format!("({d} // 7)"),
            "days" => d.into(),
            "daysofweek" => format!("({d} % 7)"),
            "hours" => format!("({s} // 3600)"),
            "minutes" => format!("({s} // 60)"),
            "seconds" => s.into(),
            "minutesofhour" => format!("({s} // 60 % 60)"),
            "secondsofminute" => format!("({s} % 60)"),
            "secondsofhour" => format!("({s} % 3600)"),
            "millisecondsofsecond" => format!("({n} // 1000000)"),
            "microsecondsofsecond" => format!("({n} // 1000)"),
            "nanosecondsofsecond" => n.into(),
            "milliseconds" | "microseconds" | "nanoseconds" => {
                let divisor = match key.as_str() {
                    "milliseconds" => 1000000,
                    "microseconds" => 1000,
                    _ => 1,
                };
                format!(
                    "CAST((CAST({s} AS HUGEINT) * 1000000000 + {n}) // {divisor} AS DECIMAL(38, 0))"
                )
            }
            _ => "NULL".into(),
        });
    }
    if matches!(key.as_str(), "timezone" | "offset") {
        return Some(if k == "datetime" && key == "timezone" {
            format!(
                "COALESCE(NULLIF(regexp_extract({}, '\\[([^]]+)\\]$', 1), ''), {})",
                payload(),
                offset_text(k)
            )
        } else if matches!(k, "time" | "datetime") {
            offset_text(k)
        } else {
            "NULL".into()
        });
    }
    if matches!(key.as_str(), "offsetminutes" | "offsetseconds") {
        return Some(if matches!(k, "time" | "datetime") {
            if key == "offsetminutes" {
                format!("({} // 60)", offset(k))
            } else {
                offset(k)
            }
        } else {
            "NULL".into()
        });
    }
    if matches!(key.as_str(), "epochseconds" | "epochmillis") {
        return Some(if k == "datetime" {
            if key == "epochseconds" {
                epoch_seconds()
            } else {
                format!("({} * 1000 + {} // 1000000)", epoch_seconds(), nanos(k))
            }
        } else {
            "NULL".into()
        });
    }
    let clock_component = match key.as_str() {
        "hour" => Some(format!("({} // 3600)", seconds(k))),
        "minute" => Some(format!("({} // 60 % 60)", seconds(k))),
        "second" => Some(format!("({} % 60)", seconds(k))),
        "millisecond" => Some(format!("({} // 1000000)", nanos(k))),
        "microsecond" => Some(format!("({} // 1000)", nanos(k))),
        "nanosecond" => Some(nanos(k)),
        _ => None,
    };
    if let Some(value) = clock_component {
        return Some(if k == "date" { "NULL".into() } else { value });
    }
    if !matches!(k, "date" | "localdatetime" | "datetime") {
        return Some("NULL".into());
    }
    let year = date_field(1);
    let representative = format!("(2000 + {})", modulo(&year, 400));
    let date = if native_date {
        "__arg0".into()
    } else {
        format!(
            "make_date({representative}, {}, {})",
            date_field(2),
            date_field(3)
        )
    };
    if !native_date && key == "year" {
        return Some(year);
    }
    if !native_date && key == "weekyear" {
        return Some(format!(
            "(CAST(date_part('isoyear', {date}) AS BIGINT) + {year} - {representative})"
        ));
    }
    let unit = match key.as_str() {
        "year" => "year",
        "month" => "month",
        "day" | "dayofmonth" => "day",
        "ordinalday" | "dayofyear" => "doy",
        "week" => "week",
        "weekyear" => "isoyear",
        "dayofweek" | "weekday" => "isodow",
        "quarter" => "quarter",
        "dayofquarter" => {
            return Some(format!(
                "(date_diff('day', date_trunc('quarter', {date}), {date}) + 1)"
            ));
        }
        _ => return Some("NULL".into()),
    };
    Some(format!("CAST(date_part('{unit}', {date}) AS BIGINT)"))
}

pub(in crate::ir::rel) fn lower(
    context: &LoweringContext<'_>,
    plan: &LogicalPlan,
    expr: &IrExpr,
) -> Option<Expr> {
    if let IrExpr::Call { name, args } = expr
        && name.starts_with("cypher_temporal.")
    {
        return constructor(context, plan, name, args);
    }
    if let IrExpr::Call { name, args } = expr
        && args.len() == 2
    {
        let op = match name.as_str() {
            "cypher_eq" => Some(BinaryOp::Eq),
            "cypher_neq" => Some(BinaryOp::Neq),
            "cypher_lt" => Some(BinaryOp::Lt),
            "cypher_lte" => Some(BinaryOp::Lte),
            "cypher_gt" => Some(BinaryOp::Gt),
            "cypher_gte" => Some(BinaryOp::Gte),
            _ => None,
        };
        if let Some(op) = op {
            return native_timestamp_comparison(context, plan, op, &args[0], &args[1]);
        }
    }
    if let IrExpr::Binary { op, lhs, rhs } = expr {
        return native_timestamp_comparison(context, plan, *op, lhs, rhs)
            .or_else(|| arithmetic(context, plan, *op, lhs, rhs));
    }
    let (binding, name) = match expr {
        IrExpr::Property { binding, name, .. } => (binding, name),
        IrExpr::Call { name, args } if name == "cypher_subscript" => match args.as_slice() {
            [IrExpr::Binding(binding), IrExpr::Lit(Lit::String(name))] => (binding, name),
            _ => return None,
        },
        _ => return None,
    };
    // Graph property reads are already typed columns; component access applies
    // only when the binding itself is a temporal scalar.
    let ty = binding_type(plan, binding)?;
    let k = kind(ty)?;
    let column = resolve_column_name(plan, binding)?;
    let sql = component_sql(k, transport::is_date_type(ty), name)?;
    let wide = k == "duration"
        && matches!(
            name.to_ascii_lowercase().as_str(),
            "milliseconds" | "microseconds" | "nanoseconds"
        );
    let output = if wide {
        domain::data_type("cypher_integer", DataType::Decimal128(38, 0))
    } else if matches!(name.to_ascii_lowercase().as_str(), "timezone" | "offset") {
        DataType::Utf8
    } else {
        DataType::Int64
    };
    let out = output.clone();
    let key = name.clone();
    Some(function(
        &format!("cypher_temporal_component_{}", name.to_ascii_lowercase()),
        vec![col_exact(column)],
        vec![ty.clone()],
        output,
        null_guard(sql),
        move |args| {
            if args[0].is_null() {
                return ScalarValue::try_from(&out);
            }
            let value = transport::decode(&args[0])?.component(&key);
            if wide {
                let integer = match value {
                    Value::Int(n) | Value::Long(n) => i128::from(n),
                    Value::BigInt(n) => n
                        .to_string()
                        .parse::<i128>()
                        .map_err(|e| DataFusionError::Execution(e.to_string()))?,
                    _ => return ScalarValue::try_from(&out),
                };
                return domain::scalar(
                    "cypher_integer",
                    ScalarValue::Decimal128(Some(integer), 38, 0),
                );
            }
            match value {
                Value::Int(n) | Value::Long(n) => Ok(ScalarValue::Int64(Some(n))),
                Value::String(s) => Ok(ScalarValue::Utf8(Some(s))),
                Value::Null => ScalarValue::try_from(&out),
                _ => Err(DataFusionError::Execution(
                    "Temporal component exceeds SQL result range".into(),
                )),
            }
        },
    ))
}

// Bridge a native timestamp column and a constant Cypher temporal value without
// converting the column to text or rounding the bound to the wrong side. Native
// zoned timestamps represent instants; unzoned timestamps represent local time.
fn native_timestamp_comparison(
    context: &LoweringContext<'_>,
    plan: &LogicalPlan,
    mut op: BinaryOp,
    lhs: &IrExpr,
    rhs: &IrExpr,
) -> Option<Expr> {
    use arrow::datatypes::TimeUnit;
    if !matches!(
        op,
        BinaryOp::Eq | BinaryOp::Neq | BinaryOp::Lt | BinaryOp::Lte | BinaryOp::Gt | BinaryOp::Gte
    ) {
        return None;
    }
    let mut column = operand(context, plan, lhs)?;
    let mut constant = operand(context, plan, rhs)?;
    if !matches!(
        column.get_type(plan.schema()).ok()?,
        DataType::Timestamp(..)
    ) {
        std::mem::swap(&mut column, &mut constant);
        op = match op {
            BinaryOp::Lt => BinaryOp::Gt,
            BinaryOp::Lte => BinaryOp::Gte,
            BinaryOp::Gt => BinaryOp::Lt,
            BinaryOp::Gte => BinaryOp::Lte,
            other => other,
        };
    }
    let DataType::Timestamp(unit, zone) = column.get_type(plan.schema()).ok()? else {
        return None;
    };
    let temporal = if let Some(Value::Temporal(value)) = native_values::literal_value(&constant) {
        value
    } else {
        let value = match constant {
            Expr::Literal(value, _) => value,
            Expr::Column(c) => match temporal_constant_bindings(plan).get(&c.name)? {
                Value::Temporal(value) => transport::scalar(value).ok()?,
                _ => return None,
            },
            _ => return None,
        };
        if !transport::is_type(&value.data_type()) { return None; }
        if value.is_null() { return Some(lit(ScalarValue::Boolean(None))); }
        transport::decode(&value).ok()?
    };
    let value = match temporal {
        TemporalValue::DateTime(value, _) if zone.is_some() => value.naive_utc(),
        TemporalValue::LocalDateTime(value) if zone.is_none() => value,
        _ => return None,
    }
    .and_utc();
    let ns =
        i128::from(value.timestamp()) * 1_000_000_000 + i128::from(value.timestamp_subsec_nanos());
    let tick = match unit {
        TimeUnit::Second => 1_000_000_000,
        TimeUnit::Millisecond => 1_000_000,
        TimeUnit::Microsecond => 1000,
        TimeUnit::Nanosecond => 1,
    };
    let floor = ns.div_euclid(tick);
    let remainder = ns.rem_euclid(tick);
    if remainder != 0 && matches!(op, BinaryOp::Eq | BinaryOp::Neq) {
        // Preserve SQL NULL while rejecting equality to an unrepresentable time.
        return Some(if op == BinaryOp::Eq {
            column.clone().not_eq(column)
        } else {
            column.clone().eq(column)
        });
    }
    let bound = if matches!(op, BinaryOp::Gte | BinaryOp::Lt) && remainder != 0 {
        floor + 1
    } else {
        floor
    };
    let bound = i64::try_from(bound).ok()?;
    let literal = lit(match unit {
        TimeUnit::Second => ScalarValue::TimestampSecond(Some(bound), zone),
        TimeUnit::Millisecond => ScalarValue::TimestampMillisecond(Some(bound), zone),
        TimeUnit::Microsecond => ScalarValue::TimestampMicrosecond(Some(bound), zone),
        TimeUnit::Nanosecond => ScalarValue::TimestampNanosecond(Some(bound), zone),
    });
    Some(match op {
        BinaryOp::Eq => column.eq(literal),
        BinaryOp::Neq => column.not_eq(literal),
        BinaryOp::Lt => column.lt(literal),
        BinaryOp::Lte => column.lt_eq(literal),
        BinaryOp::Gt => column.gt(literal),
        BinaryOp::Gte => column.gt_eq(literal),
        _ => return None,
    })
}

fn expression_kind(plan: &LogicalPlan, expr: &Expr) -> Option<String> {
    let ty = expr.get_type(plan.schema()).ok()?;
    if let Some(k) = kind(&ty) {
        return Some(k.into());
    }
    if let Expr::Column(column) = expr
        && let Some(values) = finite_values(plan, &column.name)
    {
        let kinds = values
            .iter()
            .filter(|v| !v.is_null())
            .map(|v| transport::decode(v).ok().map(|v| v.kind()))
            .collect::<Option<Vec<_>>>()?;
        if let Some(first) = kinds.first()
            && kinds.iter().all(|kind| kind == first)
        {
            return Some((*first).into());
        }
    }
    let value = match expr {
        Expr::Literal(value, _) => Some(value.clone()),
        Expr::Column(c) => temporal_constant_bindings(plan).get(&c.name).and_then(|v| {
            if let Value::Temporal(t) = v {
                transport::scalar(t).ok()
            } else {
                None
            }
        }),
        _ => None,
    }?;
    transport::decode(&value).ok().map(|v| v.kind().into())
}
fn duration_part(arg: &str, part: usize) -> String {
    format!("CAST(split_part({arg}, '|', {part}) AS HUGEINT)")
}
fn bind_steps(steps: &[(usize, String)], result: &str) -> String {
    let mut query = "SELECT __arg0 AS __local0, __arg1 AS __local1".to_string();
    // DuckDB supports earlier aliases within a projection except aliases
    // containing error(). Flush those guards before any dependent expression.
    // Small stages bound parser depth and avoid expanding every dependency.
    let mut fields = Vec::new();
    for (n, expr) in steps {
        fields.push(format!("{expr} AS __local{n}"));
        if fields.len() == 4 || expr.contains("error(") {
            query = format!(
                "SELECT *, {} FROM ({query}) AS __local99",
                fields.join(", ")
            );
            fields.clear();
        }
    }
    if !fields.is_empty() {
        query = format!(
            "SELECT *, {} FROM ({query}) AS __local99",
            fields.join(", ")
        );
    }
    format!("(SELECT {result} FROM ({query}) AS __local99)")
}
fn normalized_duration_sql(months: String, days: String, nanos: String) -> String {
    let steps = vec![
        (2, months),
        (3, days),
        (4, nanos),
        (
            5,
            "((__local4 % 1000000000 + 1000000000) % 1000000000)".into(),
        ),
    ];
    bind_steps(
        &steps,
        "'duration|' || CAST(CAST(__local2 AS BIGINT) AS VARCHAR) || '|' || CAST(CAST(__local3 AS BIGINT) AS VARCHAR) || '|' || CAST(CAST((__local4 - __local5) // 1000000000 AS BIGINT) AS VARCHAR) || '|' || CAST(__local5 AS VARCHAR)",
    )
}
fn numeric_ratio(expr: &Expr) -> Option<(i64, i64)> {
    let Expr::Literal(v, _) = expr else {
        return None;
    };
    let text = match v {
        ScalarValue::Int64(Some(n)) => n.to_string(),
        ScalarValue::Float64(Some(n)) if n.is_finite() => n.to_string(),
        _ => return None,
    };
    let (whole, fraction) = text.split_once('.').unwrap_or((&text, ""));
    if fraction.len() > 9 {
        return None;
    }
    let den = 10_i64.pow(fraction.len() as u32);
    let numerator = whole.parse::<i64>().ok()?.checked_mul(den)?.checked_add(
        fraction.parse::<i64>().unwrap_or(0) * if text.starts_with('-') { -1 } else { 1 },
    )?;
    if numerator.unsigned_abs() > 1_000_000_000 {
        return None;
    }
    Some((numerator, den))
}
fn arithmetic(
    context: &LoweringContext<'_>,
    plan: &LogicalPlan,
    op: BinaryOp,
    left: &IrExpr,
    right: &IrExpr,
) -> Option<Expr> {
    if !matches!(
        op,
        BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div
    ) {
        return None;
    }
    let mut lhs = operand(context, plan, left)?;
    let mut rhs = operand(context, plan, right)?;
    let mut lk = expression_kind(plan, &lhs);
    let mut rk = expression_kind(plan, &rhs);
    if op == BinaryOp::Add
        && lk.as_deref() == Some("duration")
        && rk.as_deref().is_some_and(|k| k != "duration")
        || op == BinaryOp::Mul && lk.is_none() && rk.as_deref() == Some("duration")
    {
        std::mem::swap(&mut lhs, &mut rhs);
        std::mem::swap(&mut lk, &mut rk);
    }
    let k = lk.as_deref()?;
    let sign = if op == BinaryOp::Sub { -1 } else { 1 };
    let (sql, result_kind) = if k == "duration"
        && rk.as_deref() == Some("duration")
        && matches!(op, BinaryOp::Add | BinaryOp::Sub)
    {
        let add = |part| {
            if part == 2 || part == 3 {
                format!(
                    "({} + CAST({sign} * {} AS BIGINT))",
                    duration_part("__local0", part),
                    duration_part("__local1", part)
                )
            } else {
                format!(
                    "({} + {sign} * {})",
                    duration_part("__local0", part),
                    duration_part("__local1", part)
                )
            }
        };
        (
            normalized_duration_sql(
                add(2),
                add(3),
                format!("({} * 1000000000 + {})", add(4), add(5)),
            ),
            "duration",
        )
    } else if k == "duration" && matches!(op, BinaryOp::Mul | BinaryOp::Div) {
        let (mut n, mut d) = numeric_ratio(&rhs).or_else(|| {
            let source = if matches!(expression_kind(plan, &rhs), None)
                && op == BinaryOp::Mul
                && expression_kind(plan, &operand(context, plan, left)?).is_none()
            {
                left
            } else {
                right
            };
            let value = super::super::constants::constant_value_expr(source).ok()??;
            numeric_ratio(&super::super::constants::value_literal_expr(&value).ok()?)
        })?;
        if op == BinaryOp::Div {
            if n == 0 {
                return None;
            }
            std::mem::swap(&mut n, &mut d);
        }
        if d < 0 {
            n = -n;
            d = -d;
        }
        let steps = vec![
            (2, format!("({} * {n})", duration_part("__local0", 2))),
            (3, format!("(__local2 // {d})")),
            (
                4,
                format!(
                    "({} * {n} * 86400 + (__local2 - __local3 * {d}) * 2629746)",
                    duration_part("__local0", 3)
                ),
            ),
            (5, format!("(__local4 // ({d} * 86400))")),
            (
                6,
                format!(
                    "((({} * 1000000000 + {}) * {n} + (__local4 - __local5 * {d} * 86400) * 1000000000) // {d})",
                    duration_part("__local0", 4),
                    duration_part("__local0", 5)
                ),
            ),
            (
                7,
                "((__local6 % 1000000000 + 1000000000) % 1000000000)".into(),
            ),
        ];
        (
            bind_steps(
                &steps,
                "'duration|' || CAST(CAST(__local3 AS BIGINT) AS VARCHAR) || '|' || CAST(CAST(__local5 AS BIGINT) AS VARCHAR) || '|' || CAST(CAST((__local6 - __local7) // 1000000000 AS BIGINT) AS VARCHAR) || '|' || CAST(__local7 AS VARCHAR)",
            ),
            "duration",
        )
    } else if matches!(k, "localtime" | "time")
        && rk.as_deref() == Some("duration")
        && matches!(op, BinaryOp::Add | BinaryOp::Sub)
    {
        let seconds = seconds(k).replace("__arg0", "__local0");
        let nanos = nanos(k).replace("__arg0", "__local0");
        let steps = vec![
            (
                2,
                format!(
                    "({seconds} * CAST(1000000000 AS HUGEINT) + {nanos} + {sign} * ({} * 1000000000 + {}))",
                    duration_part("__local1", 4),
                    duration_part("__local1", 5)
                ),
            ),
            (
                3,
                "((__local2 % 86400000000000 + 86400000000000) % 86400000000000)".into(),
            ),
            (4, "(__local3 // 1000000000)".into()),
            (5, "(__local3 % 1000000000)".into()),
        ];
        let tail = if k == "time" {
            format!(" || {}", offset_text(k).replace("__arg0", "__local0"))
        } else {
            String::new()
        };
        (
            bind_steps(
                &steps,
                &format!(
                    "'{k}|' || printf('%02d:%02d', CAST(__local4 // 3600 AS BIGINT), CAST(__local4 // 60 % 60 AS BIGINT)) || CASE WHEN __local4 % 60 <> 0 OR __local5 <> 0 THEN printf(':%02d', CAST(__local4 % 60 AS BIGINT)) ELSE '' END || CASE WHEN __local5 <> 0 THEN '.' || rtrim(printf('%09d', CAST(__local5 AS BIGINT)), '0') ELSE '' END{tail}"
                ),
            ),
            k,
        )
    } else if matches!(k, "date" | "localdatetime" | "datetime")
        && rk.as_deref() == Some("duration")
        && matches!(op, BinaryOp::Add | BinaryOp::Sub)
    {
        let ty = lhs.get_type(plan.schema()).ok()?;
        if k == "datetime" {
            let fixed = domain::descriptor(&ty)
                .is_some_and(|(name, _)| name == "cypher_temporal_datetime_fixed")
                || match &lhs {
                    Expr::Literal(v, _) => matches!(
                        transport::decode(v).ok(),
                        Some(TemporalValue::DateTime(_, None))
                    ),
                    Expr::Column(c) => finite_values(plan, &c.name).is_some_and(|vs| {
                        vs.iter().all(|v| {
                            v.is_null()
                                || matches!(
                                    transport::decode(v).ok(),
                                    Some(TemporalValue::DateTime(_, None))
                                )
                        })
                    }),
                    _ => false,
                };
            if !fixed {
                return None;
            }
        }
        (
            calendar_sql(k, transport::is_date_type(&ty), sign),
            if k == "datetime" { "datetime_fixed" } else { k },
        )
    } else {
        return None;
    };
    let sql = if k != "duration" {
        let seconds = duration_part("__arg1", 4);
        let ns = duration_part("__arg1", 5);
        let months = duration_part("__arg1", 2);
        let days = duration_part("__arg1", 3);
        format!(
            "CASE WHEN {sign} * {seconds} NOT BETWEEN -9223372036854775 AND 9223372036854775 OR {sign} * ({seconds} * 1000000000 + {ns}) NOT BETWEEN -9223372036854775807000000 AND 9223372036854775807000000 OR {sign} * {months} NOT BETWEEN -9223372036854775808 AND 9223372036854775807 OR {sign} * {days} NOT BETWEEN -9223372036854775808 AND 9223372036854775807 THEN error('Temporal arithmetic overflow') ELSE {sql} END"
        )
    } else {
        sql
    };
    let ty = if result_kind == "date" && transport::is_date_type(&lhs.get_type(plan.schema()).ok()?)
    {
        domain::data_type("cypher_temporal", DataType::Date32)
    } else {
        domain::data_type(&format!("cypher_temporal_{result_kind}"), DataType::Utf8)
    };
    let output = ty.clone();
    let inputs = vec![
        lhs.get_type(plan.schema()).ok()?,
        rhs.get_type(plan.schema()).ok()?,
    ];
    Some(function(
        "cypher_typed_temporal_arithmetic",
        vec![lhs, rhs],
        inputs,
        ty,
        bind_operands(
            format!("CASE WHEN __arg0 IS NULL OR __arg1 IS NULL THEN NULL ELSE {sql} END"),
            2,
        ),
        move |args| {
            if args.iter().any(ScalarValue::is_null) {
                return ScalarValue::try_from(&output);
            }
            let value = |a: &ScalarValue| {
                if transport::is_type(&a.data_type()) {
                    transport::decode(a).map(Value::Temporal)
                } else {
                    crate::ir::value::scalar_semantic_value(a).ok_or_else(|| {
                        DataFusionError::Execution("Expected numeric duration factor".into())
                    })
                }
            };
            let Value::Temporal(result) =
                crate::ir::temporal::arithmetic(op, &value(&args[0])?, &value(&args[1])?)
                    .map_err(DataFusionError::Execution)?
            else {
                return Err(DataFusionError::Execution(
                    "Expected temporal result".into(),
                ));
            };
            transport::scalar_for_type(&result, &output)
        },
    ))
}

fn modulo(expr: &str, divisor: i64) -> String {
    format!("(({expr} % {divisor} + {divisor}) % {divisor})")
}
fn floor_div(expr: &str, divisor: i64) -> String {
    format!("(({expr} - {}) // {divisor})", modulo(expr, divisor))
}
fn date_text(year: &str, month: &str, day: &str) -> String {
    format!(
        "(CASE WHEN {year} < 0 THEN '-' || printf('%04d', CAST(-{year} AS BIGINT)) WHEN {year} > 9999 THEN '+' || CAST({year} AS VARCHAR) ELSE printf('%04d', CAST({year} AS BIGINT)) END || printf('-%02d-%02d', CAST({month} AS BIGINT), CAST({day} AS BIGINT)))"
    )
}
fn calendar_sql(k: &str, native_date: bool, sign: i32) -> String {
    let mut steps = Vec::new();
    for (index, part) in ["year", "month", "day"].into_iter().enumerate() {
        let value = if native_date {
            format!("CAST(date_part('{part}', __local0) AS HUGEINT)")
        } else {
            date_field(index + 1).replace("__arg0", "__local0")
        };
        steps.push((index + 2, value));
    }
    // Cypher applies calendar months, then calendar days, then elapsed time.
    // Gregorian cycles keep wide dates entirely in integer arithmetic. The
    // engine's DATE operations only see the representative years 2000..2399.
    steps.push((
        5,
        format!("CAST({sign} * {} AS BIGINT)", duration_part("__local1", 2)),
    ));
    steps.push((6, "(__local2 * 12 + __local3 - 1 + __local5)".into()));
    steps.push((7, floor_div("__local6", 12)));
    steps.push((8, format!("({} + 1)", modulo("__local6", 12))));
    steps.push((9, format!("(2000 + {})", modulo("__local7", 400))));
    steps.push((10,"make_date(CAST(__local9 AS BIGINT), CAST(__local8 AS BIGINT), CAST(least(__local4, day(last_day(make_date(CAST(__local9 AS BIGINT), CAST(__local8 AS BIGINT), 1)))) AS BIGINT))".into()));
    // Bound every calendar stage in the same order as the native implementation.
    // chrono years are [-262143,262142]; its extreme years are partial at the
    // date boundary, so compare epoch days against the exact endpoint dates.
    let min = chrono::NaiveDate::MIN.num_days_from_ce() as i64 - 719163;
    let max = chrono::NaiveDate::MAX.num_days_from_ce() as i64 - 719163;
    let wide_min = crate::ir::temporal::CalendarDate::new(-999999999, 1, 1)
        .unwrap()
        .epoch_days();
    let wide_max = crate::ir::temporal::CalendarDate::new(999999999, 12, 31)
        .unwrap()
        .epoch_days();
    steps.push((11,"(date_diff('day', DATE '1970-01-01', __local10) + ((__local7 - __local9) // 400) * 146097)".into()));
    let in_chrono = "(__local2 BETWEEN -262143 AND 262142)";
    let guard = |expr: &str| {
        format!(
            "CASE WHEN {expr} IS NULL THEN NULL WHEN {expr} BETWEEN CASE WHEN {in_chrono} THEN {min} ELSE {wide_min} END AND CASE WHEN {in_chrono} THEN {max} ELSE {wide_max} END THEN {expr} ELSE error('Calendar shift out of range') END"
        )
    };
    steps.push((12,format!("CASE WHEN {in_chrono} AND abs(CAST(__local5 AS HUGEINT)) > 4294967295 THEN error('Month shift out of range') ELSE {} END",guard("__local11"))));
    steps.push((
        13,
        format!(
            "(__local12 + CAST({sign} * {} AS BIGINT))",
            duration_part("__local1", 3)
        ),
    ));
    steps.push((14, guard("__local13")));
    if k == "date" {
        steps.push((
            15,
            format!(
                "(__local14 + CAST({sign} * {} AS BIGINT) // 86400)",
                duration_part("__local1", 4)
            ),
        ));
    } else {
        steps.push((
            15,
            format!(
                "({} * CAST(1000000000 AS HUGEINT) + {} + {sign} * ({} * 1000000000 + {}))",
                seconds(k).replace("__arg0", "__local0"),
                nanos(k).replace("__arg0", "__local0"),
                duration_part("__local1", 4),
                duration_part("__local1", 5)
            ),
        ));
        steps.push((16, modulo("__local15", 86400000000000)));
        steps.push((
            17,
            format!("(__local14 + {})", floor_div("__local15", 86400000000000)),
        ));
    }
    steps.push((
        18,
        guard(if k == "date" {
            "__local15"
        } else {
            "__local17"
        }),
    ));
    steps.push((19, floor_div("(__local18 - 10957)", 146097)));
    steps.push((
        20,
        format!(
            "(DATE '2000-01-01' + CAST({} AS INTEGER))",
            modulo("(__local18 - 10957)", 146097)
        ),
    ));
    steps.push((21, "(year(__local20) + __local19 * 400)".into()));
    let mut result = if k == "date" && native_date {
        "make_date(CAST(__local21 AS BIGINT), CAST(month(__local20) AS BIGINT), CAST(day(__local20) AS BIGINT))".into()
    } else {
        format!(
            "'{k}|' || {}",
            date_text("__local21", "month(__local20)", "day(__local20)")
        )
    };
    if k != "date" {
        steps.push((22, "(__local16 // 1000000000)".into()));
        steps.push((23, "(__local16 % 1000000000)".into()));
        result.push_str(" || printf('T%02d:%02d', CAST(__local22 // 3600 AS BIGINT), CAST(__local22 // 60 % 60 AS BIGINT)) || CASE WHEN __local22 % 60 <> 0 OR __local23 <> 0 THEN printf(':%02d', CAST(__local22 % 60 AS BIGINT)) ELSE '' END || CASE WHEN __local23 <> 0 THEN '.' || rtrim(printf('%09d', CAST(__local23 AS BIGINT)), '0') ELSE '' END");
        if k == "datetime" {
            result.push_str(&format!(
                " || {}",
                offset_text(k).replace("__arg0", "__local0")
            ));
        }
    }
    bind_steps(&steps, &result)
}

#[cfg(all(test, feature = "duckdb"))]
mod tests {
    use super::*;
    #[test]
    fn wide_integer_domain_decodes_to_language_integer_and_preserves_null() {
        let integer = 9223372036854775807999999999_i128;
        let value = domain::scalar(
            "cypher_integer",
            ScalarValue::Decimal128(Some(integer), 38, 0),
        )
        .unwrap();
        let batch = RecordBatch::try_new(
            Arc::new(arrow::datatypes::Schema::new(vec![
                arrow::datatypes::Field::new("ns", value.data_type(), true),
            ])),
            vec![
                ScalarValue::iter_to_array(vec![
                    value.clone(),
                    ScalarValue::try_from(&value.data_type()).unwrap(),
                ])
                .unwrap(),
            ],
        )
        .unwrap();
        let (_, rows) = crate::ir::exec::batch_to_bindings(&crate::ir::runtime::ReturnedBatches {
            fields: vec!["ns".into()],
            result_form: crate::ir::policy::ResultForm::RowSet,
            batch,
        })
        .unwrap();
        assert_eq!(rows[0][0], Value::BigInt(integer.into()));
        assert_eq!(rows[1][0], Value::Null);
        let Value::BigInt(integer) = &rows[0][0] else {
            panic!()
        };
        assert_eq!(integer.to_string(), "9223372036854775807999999999");
    }

    fn quote(s: &str) -> String {
        format!("'{}'", s.replace('\'', "''"))
    }
    fn execute(sql: &str, left: &str, right: &str) -> Result<String, String> {
        let conn = duckdb::Connection::open_in_memory().unwrap();
        let sql = sql
            .replace("__arg0", &quote(left))
            .replace("__arg1", &quote(right));
        conn.query_row(&format!("SELECT CAST(({sql}) AS VARCHAR)"), [], |row| {
            row.get::<_, String>(0)
        })
        .map_err(|e| format!("{e}\n{sql}"))
    }
    #[test]
    fn typed_components_match_calendar_clock_zone_and_duration_reference() {
        for (kind, text) in [
            ("date", "2000-01-01"),
            ("date", "-1000000-12-31"),
            ("date", "+1000000-01-01"),
            ("localtime", "12:31:14.645876123"),
            ("time", "00:00:00.000000001-00:01:02"),
            ("localdatetime", "1984-11-11T12:31:14.645876123"),
            ("localdatetime", "-1000000-12-31T23:59:59.999999999"),
            ("datetime", "1969-12-31T23:59:59.999999999Z"),
            (
                "datetime",
                "1984-11-11T12:31:14.645876123+01:00[Europe/Stockholm]",
            ),
            ("duration", "P12Y5M14DT16H13M10.000000001S"),
            ("duration", "P-1M-8DT-1.000000001S"),
        ] {
            let value = crate::ir::temporal::parse(kind, text).unwrap();
            for key in [
                "year",
                "quarter",
                "month",
                "week",
                "weekYear",
                "day",
                "ordinalDay",
                "weekDay",
                "dayOfQuarter",
                "hour",
                "minute",
                "second",
                "millisecond",
                "microsecond",
                "nanosecond",
                "timezone",
                "offset",
                "offsetMinutes",
                "offsetSeconds",
                "epochSeconds",
                "epochMillis",
                "years",
                "quarters",
                "months",
                "weeks",
                "days",
                "hours",
                "minutes",
                "seconds",
                "milliseconds",
                "microseconds",
                "nanoseconds",
                "quartersOfYear",
                "monthsOfQuarter",
                "monthsOfYear",
                "daysOfWeek",
                "minutesOfHour",
                "secondsOfMinute",
                "millisecondsOfSecond",
                "microsecondsOfSecond",
                "nanosecondsOfSecond",
            ] {
                let expected = value.component(key);
                let sql = component_sql(kind, false, key).unwrap();
                if matches!(expected, Value::Null) {
                    continue;
                }
                let actual = execute(&sql, &value.encode(), "")
                    .unwrap_or_else(|e| panic!("{kind} {key}: {e}"));
                let expected = match expected {
                    Value::String(s) => s,
                    Value::Int(n) => n.to_string(),
                    Value::BigInt(n) => n.to_string(),
                    other => panic!("{other:?}"),
                };
                assert_eq!(actual, expected, "{kind} {text} {key}");
            }
        }
    }
    #[test]
    fn runtime_calendar_arithmetic_matches_reference_for_month_clamping_and_nanoseconds() {
        for (k, text) in [
            ("date", "2000-01-31"),
            ("date", "0001-01-01"),
            ("date", "-1000000-02-29"),
            ("date", "+1000000-12-31"),
            ("localdatetime", "2000-01-31T23:59:59.999999999"),
            ("localdatetime", "-1000000-02-29T00:00:00.000000001"),
            ("datetime", "1984-10-11T12:31:14.000000001+01:00"),
        ] {
            let value = crate::ir::temporal::parse(k, text).unwrap();
            for duration in [
                "P1M",
                "P-1M",
                "P1DT0.000000002S",
                "P-1DT-0.000000002S",
                "P12Y5M14DT16H13M10.000000002S",
                "PT-0.000000001S",
            ] {
                let duration = crate::ir::temporal::parse("duration", duration).unwrap();
                for (op, sign) in [(BinaryOp::Add, 1), (BinaryOp::Sub, -1)] {
                    let Value::Temporal(expected) = crate::ir::temporal::arithmetic(
                        op,
                        &Value::Temporal(value.clone()),
                        &Value::Temporal(duration.clone()),
                    )
                    .unwrap() else {
                        panic!()
                    };
                    let actual = execute(
                        &calendar_sql(k, false, sign),
                        &value.encode(),
                        &duration.encode(),
                    )
                    .unwrap_or_else(|e| panic!("{k} {text} {op:?}: {e}"));
                    assert_eq!(actual, expected.encode(), "{k} {text} {op:?} {duration}");
                }
            }
        }
    }
}

// Literal component maps still execute their constructor in SQL when constant
// folding is disabled. Validation proves the constructor's input grammar/range;
// its result is not substituted into the emitted expression.
fn constructor(
    context: &LoweringContext<'_>,
    plan: &LogicalPlan,
    name: &str,
    args: &[IrExpr],
) -> Option<Expr> {
    let k = name.strip_prefix("cypher_temporal.")?;
    if !matches!(
        k,
        "date" | "localtime" | "time" | "localdatetime" | "datetime" | "duration"
    ) || args.len() != 1
    {
        return None;
    }
    let IrExpr::Call {
        name: map,
        args: fields,
    } = &args[0]
    else {
        return None;
    };
    if map != "map" || fields.len() % 2 != 0 {
        return None;
    }
    let mut values = BTreeMap::new();
    let mut arguments = Vec::new();
    let mut expressions = BTreeMap::new();
    for pair in fields.chunks_exact(2) {
        let IrExpr::Lit(Lit::String(key)) = &pair[0] else {
            return None;
        };
        let value = super::super::constants::constant_value_expr(&pair[1]).ok()??;
        if !matches!(&value, Value::Int(_) | Value::Long(_) | Value::String(_)) {
            return None;
        }
        let lowered = context.lower_expr(plan, &pair[1]).ok()?;
        expressions.insert(
            key.to_ascii_lowercase(),
            format!("__arg{}", arguments.len()),
        );
        arguments.push(lowered);
        values.insert(key.to_ascii_lowercase(), value);
    }
    let constructed = crate::ir::temporal::construct(k, &Value::Map(values.clone())).ok()?;
    let Value::Temporal(constructed) = constructed else {
        return None;
    };
    let get = |field: &str, default: &str| {
        expressions
            .get(field)
            .cloned()
            .unwrap_or_else(|| default.into())
    };
    let has_date = matches!(k, "date" | "localdatetime" | "datetime");
    let has_time = matches!(k, "localtime" | "time" | "localdatetime" | "datetime");
    let allowed = if k == "duration" {
        vec![
            "years",
            "months",
            "weeks",
            "days",
            "hours",
            "minutes",
            "seconds",
            "milliseconds",
            "microseconds",
            "nanoseconds",
        ]
    } else {
        let mut allowed = Vec::new();
        if has_date {
            allowed.extend(["year", "month", "day"]);
        }
        if has_time {
            allowed.extend([
                "hour",
                "minute",
                "second",
                "millisecond",
                "microsecond",
                "nanosecond",
            ]);
        }
        if matches!(k, "time" | "datetime") {
            allowed.push("timezone");
        }
        allowed
    };
    if expressions
        .keys()
        .any(|key| !allowed.contains(&key.as_str()))
    {
        return None;
    }
    let mut output_kind = k;
    let sql = if k == "duration" {
        let months = format!("(CAST({} AS HUGEINT) * 12 + CAST({} AS HUGEINT))", get("years", "0"), get("months", "0"));
        let days = format!("(CAST({} AS HUGEINT) * 7 + CAST({} AS HUGEINT))", get("weeks", "0"), get("days", "0"));
        let ns = format!(
            "((CAST({} AS HUGEINT) * 3600 + CAST({} AS HUGEINT) * 60 + CAST({} AS HUGEINT)) * 1000000000 + CAST({} AS HUGEINT) * 1000000 + CAST({} AS HUGEINT) * 1000 + CAST({} AS HUGEINT))",
            get("hours", "0"),
            get("minutes", "0"),
            get("seconds", "0"),
            get("milliseconds", "0"),
            get("microseconds", "0"),
            get("nanoseconds", "0")
        );
        // Local aliases bind only computed constructor components, with no
        // relation scans, sorting, or per-input-row cardinality expansion.
        format!(
            "(SELECT 'duration|' || CAST(CAST({months} AS BIGINT) AS VARCHAR) || '|' || CAST(CAST({days} AS BIGINT) AS VARCHAR) || '|' || CAST(CAST((__local50 - {}) // 1000000000 AS BIGINT) AS VARCHAR) || '|' || CAST({} AS VARCHAR) FROM (SELECT {ns} AS __local50) AS __local99)",
            modulo("__local50", 1000000000),
            modulo("__local50", 1000000000)
        )
    } else {
        if has_date
            && !matches!(constructed.calendar_date(),Some(d) if (1..=9999).contains(&d.year))
        {
            return None;
        }
        let date = format!(
            "make_date({}, {}, {})",
            get("year", "0"),
            get("month", "1"),
            get("day", "1")
        );
        if k == "date" {
            date
        } else {
            let n = format!(
                "({} * 1000000 + {} * 1000 + {})",
                get("millisecond", "0"),
                get("microsecond", "0"),
                get("nanosecond", "0")
            );
            let h = get("hour", "0");
            let m = get("minute", "0");
            let s = get("second", "0");
            let clock = format!(
                "printf('%02d:%02d', CAST({h} AS BIGINT), CAST({m} AS BIGINT)) || CASE WHEN {s} <> 0 OR {n} <> 0 THEN printf(':%02d', CAST({s} AS BIGINT)) ELSE '' END || CASE WHEN {n} <> 0 THEN '.' || rtrim(printf('%09d', CAST({n} AS BIGINT)), '0') ELSE '' END"
            );
            let prefix = if has_date {
                format!("'{k}|' || CAST({date} AS VARCHAR) || 'T' || ")
            } else {
                format!("'{k}|' || ")
            };
            let mut sql = format!("{prefix}{clock}");
            if matches!(k, "time" | "datetime") {
                // Named zones and defaults read timezone policy; only explicit
                // canonical numeric offsets are a portable SQL constructor.
                let Some(Value::String(zone)) = values.get("timezone") else {
                    return None;
                };
                let canonical = match &constructed {
                    TemporalValue::Time(_, _) | TemporalValue::DateTime(_, None) => {
                        match constructed.component("offset") {
                            Value::String(s) => s,
                            _ => return None,
                        }
                    }
                    _ => return None,
                };
                if zone != &canonical {
                    return None;
                }
                sql.push_str(&format!(" || {}", get("timezone", "'Z'")));
                if k == "datetime" {
                    output_kind = "datetime_fixed";
                }
            }
            sql
        }
    };
    let out = if k == "date" {
        domain::data_type("cypher_temporal", DataType::Date32)
    } else {
        domain::data_type(&format!("cypher_temporal_{output_kind}"), DataType::Utf8)
    };
    let result_type = out.clone();
    // The argument order follows the source map, which need not be sorted.
    let keys = fields
        .chunks_exact(2)
        .map(|p| {
            if let IrExpr::Lit(Lit::String(s)) = &p[0] {
                s.to_ascii_lowercase()
            } else {
                unreachable!()
            }
        })
        .collect::<Vec<_>>();
    let kind_name = k.to_owned();
    let types = arguments
        .iter()
        .map(|a| a.get_type(plan.schema()).ok())
        .collect::<Option<Vec<_>>>()?;
    Some(function(
        "cypher_runtime_temporal_components",
        arguments,
        types,
        out,
        sql,
        move |args| {
            let fields = keys
                .iter()
                .zip(args)
                .map(|(key, value)| {
                    Some((key.clone(), crate::ir::value::scalar_semantic_value(value)?))
                })
                .collect::<Option<BTreeMap<_, _>>>()
                .ok_or_else(|| {
                    DataFusionError::Execution("Invalid temporal constructor argument".into())
                })?;
            let Value::Temporal(value) =
                crate::ir::temporal::construct(&kind_name, &Value::Map(fields))
                    .map_err(DataFusionError::Execution)?
            else {
                return Err(DataFusionError::Execution(
                    "Expected temporal constructor result".into(),
                ));
            };
            transport::scalar_for_type(&value, &result_type)
        },
    ))
}
