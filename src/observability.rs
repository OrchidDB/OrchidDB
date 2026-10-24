//! Predicate translation shared by native HTTP and DuckDB extension adapters.
//! These are sound prefilters; callers must retain every SQL predicate.
use datafusion::common::ScalarValue;
use datafusion::logical_expr::{Expr, Operator};
type Result<T> = std::result::Result<T, String>;
pub(crate) fn range_query(
    engine: &str,
    metric: &str,
    filters: &[Expr],
    start: Option<i64>,
    end: Option<i64>,
) -> Result<(String, i64, i64)> {
    let mut bounds = Bounds { start, end };
    let mut labels = Vec::new();
    let mut lines = Vec::new();
    let mut values = Vec::new();
    for predicate in filters {
        for p in datafusion::logical_expr::utils::split_conjunction(predicate) {
            push(p, engine, &mut bounds, &mut labels, &mut lines, &mut values)?;
        }
    }
    // A parent may still provide bounds. Refuse an unbounded leaf rather
    // than materializing an entire backend when a predicate cannot lower.
    let (Some(start), Some(end)) = (bounds.start, bounds.end) else {
        return Err(format!(
            "{engine} scan requires constant timestamp lower and upper bounds"
        ));
    };
    let max_window = if engine == "loki" { 900 } else { 86400 };
    let span = end
        .checked_sub(start)
        .ok_or("invalid telemetry time bounds")?;
    if start < 0 || span <= 0 || span > max_window * 1_000_000_000 {
        return Err(format!(
            "{} requires a positive bounded window <= {max_window}s",
            engine
        ));
    }
    let mut query = if engine == "prometheus" {
        if !valid_name(metric, true) {
            return Err("invalid Prometheus metric table name".into());
        }
        format!("{metric}{{{}}}", labels.join(","))
    } else {
        if !labels.iter().any(|m| excludes_empty(m)) {
            return Err("Loki scan requires a label matcher that excludes empty labels".into());
        }
        format!("{{{}}}{}", labels.join(","), lines.join(""))
    };
    for (operator, value) in values {
        // SQL orders NaN above finite values. PromQL comparisons do not,
        // so preserve NaNs for the residual SQL predicate to decide.
        query = if matches!(operator.as_str(), ">" | ">=") {
            format!("(({query}) {operator} {value}) or (({query}) != ({query}))")
        } else {
            format!("({query}) {operator} {value}")
        };
    }
    Ok((query, start, end))
}
#[derive(Default)]
struct Bounds {
    start: Option<i64>,
    end: Option<i64>,
}
fn valid_name(s: &str, metric: bool) -> bool {
    !s.is_empty()
        && s.chars().enumerate().all(|(i, c)| {
            c == '_'
                || (metric && c == ':')
                || c.is_ascii_alphabetic()
                || (i > 0 && c.is_ascii_digit())
        })
}
fn quoted(s: &str) -> String {
    serde_json::to_string(s).expect("string serialization")
}
fn label_column(e: &Expr) -> Option<&str> {
    let Expr::Column(c) = e else {
        return None;
    };
    (valid_name(&c.name, false)
        && !matches!(
            c.name.as_str(),
            "timestamp" | "value" | "line" | "labels" | "structured_metadata" | "sample_id"
        ))
    .then_some(c.name.as_str())
}
fn excludes_empty(matcher: &str) -> bool {
    let Some(at) = matcher.find(['=', '!']) else {
        return false;
    };
    let tail = &matcher[at..];
    for op in ["=~", "!~", "!=", "="] {
        if let Some(value) = tail.strip_prefix(op) {
            let Ok(value) = serde_json::from_str::<String>(value) else {
                return false;
            };
            return match op {
                "=" => !value.is_empty(),
                "!=" => value.is_empty(),
                "=~" => regex::Regex::new(&format!("^(?:{value})$")).is_ok_and(|r| !r.is_match("")),
                "!~" => regex::Regex::new(&format!("^(?:{value})$")).is_ok_and(|r| r.is_match("")),
                _ => false,
            };
        }
    }
    false
}

fn ns(e: &Expr) -> Option<i64> {
    match e {
        Expr::Literal(ScalarValue::TimestampNanosecond(Some(n), _), _) => Some(*n),
        Expr::Literal(ScalarValue::TimestampMicrosecond(Some(n), _), _) => n.checked_mul(1000),
        Expr::Literal(ScalarValue::TimestampMillisecond(Some(n), _), _) => n.checked_mul(1_000_000),
        Expr::Literal(ScalarValue::TimestampSecond(Some(n), _), _) => n.checked_mul(1_000_000_000),
        Expr::Literal(ScalarValue::Utf8(Some(s)), _) => chrono::DateTime::parse_from_rfc3339(s)
            .ok()?
            .timestamp_nanos_opt(),
        Expr::Cast(c) => ns(&c.expr),
        _ => None,
    }
}
fn text(e: &Expr) -> Option<&str> {
    match e {
        Expr::Literal(ScalarValue::Utf8(Some(s)), _) => Some(s),
        _ => None,
    }
}
fn numeric(e: &Expr) -> Option<String> {
    let Expr::Literal(v, _) = e else {
        return None;
    };
    match v {
        ScalarValue::Float64(Some(n)) if n.is_finite() => Some(n.to_string()),
        ScalarValue::Int64(Some(n)) => Some(n.to_string()),
        _ => None,
    }
}
fn push(
    e: &Expr,
    engine: &str,
    bounds: &mut Bounds,
    labels: &mut Vec<String>,
    lines: &mut Vec<String>,
    values: &mut Vec<(String, String)>,
) -> Result<()> {
    // A guarded predicate can only be true through ELSE when every THEN is
    // false/null (e.g. Cypher's NaN guard). Its ELSE is a sound prefilter.
    if let Expr::Case(case) = e {
        if case.expr.is_none()
            && case.when_then_expr.iter().all(|(_, value)| {
                matches!(
                    value.as_ref(),
                    Expr::Literal(ScalarValue::Boolean(Some(false) | None), _)
                        | Expr::Literal(ScalarValue::Null, _)
                )
            })
        {
            if let Some(otherwise) = &case.else_expr {
                for predicate in datafusion::logical_expr::utils::split_conjunction(otherwise) {
                    push(predicate, engine, bounds, labels, lines, values)?;
                }
            }
        }
    }
    if let Expr::BinaryExpr(b) = e {
        let (column, op, value) = match (b.left.as_ref(), b.right.as_ref()) {
            (Expr::Column(c), v) => (c, b.op, v),
            (v, Expr::Column(c)) => (
                c,
                match b.op {
                    Operator::Lt => Operator::Gt,
                    Operator::LtEq => Operator::GtEq,
                    Operator::Gt => Operator::Lt,
                    Operator::GtEq => Operator::LtEq,
                    Operator::Eq => Operator::Eq,
                    Operator::NotEq => Operator::NotEq,
                    _ => return Ok(()),
                },
                v,
            ),
            _ => return Ok(()),
        };
        if column.name == "timestamp" {
            if let Some(n) = ns(value) {
                match op {
                    Operator::Gt | Operator::GtEq => {
                        bounds.start = Some(bounds.start.map_or(n, |v| v.max(n)));
                    }
                    Operator::Lt | Operator::LtEq => {
                        let n = if op == Operator::LtEq {
                            n.checked_add(1000).ok_or("timestamp overflow")?
                        } else {
                            n
                        };
                        bounds.end = Some(bounds.end.map_or(n, |v| v.min(n)));
                    }
                    _ => {}
                }
            }
        } else if column.name == "value" && engine == "prometheus" {
            if let Some(n) = numeric(value) {
                let op = match op {
                    Operator::Eq => "==",
                    Operator::Gt => ">",
                    Operator::GtEq => ">=",
                    Operator::Lt => "<",
                    Operator::LtEq => "<=",
                    _ => return Ok(()),
                };
                values.push((op.into(), n));
            }
        } else if column.name == "line" && engine == "loki" {
            // Exact line equality has a sound substring prefilter; residual
            // equality still rejects longer lines and preserves SQL semantics.
            if op == Operator::Eq
                && let Some(s) = text(value)
            {
                lines.push(format!(" |= {}", quoted(s)));
            }
        } else if valid_name(&column.name, false)
            && !matches!(
                column.name.as_str(),
                "value" | "line" | "labels" | "structured_metadata" | "sample_id"
            )
        {
            if let Some(s) = text(value) {
                match op {
                    Operator::Eq => labels.push(format!("{}={}", column.name, quoted(s))),
                    Operator::NotEq => labels.push(format!("{}!={}", column.name, quoted(s))),
                    _ => {}
                }
            }
        }
    }

    if let Expr::InList(list) = e
        && let Some(name) = label_column(&list.expr)
        && !list.list.is_empty()
    {
        if let Some(values) = list.list.iter().map(text).collect::<Option<Vec<_>>>() {
            let pattern = values
                .into_iter()
                .map(regex::escape)
                .collect::<Vec<_>>()
                .join("|");
            labels.push(format!(
                "{name}{}{}",
                if list.negated { "!~" } else { "=~" },
                quoted(&pattern)
            ));
        }
    }
    if let Expr::ScalarFunction(f) = e
        && f.args.len() == 2
        && let Some(value) = text(&f.args[1])
    {
        let escaped = regex::escape(value);
        let pattern = match f.name() {
            "starts_with" => Some(format!("(?s){escaped}.*")),
            "ends_with" => Some(format!("(?s).*{escaped}")),
            "contains" => Some(format!("(?s).*{escaped}.*")),
            _ => None,
        };
        if let Some(pattern) = pattern {
            if let Some(name) = label_column(&f.args[0]) {
                labels.push(format!("{name}=~{}", quoted(&pattern)));
            } else if engine == "loki"
                && f.name() != "contains"
                && matches!(&f.args[0],Expr::Column(c) if c.name=="line")
            {
                lines.push(format!(" |~ {}", quoted(&format!("^{pattern}$"))));
            }
        }
    }
    if engine == "loki"
        && let Expr::ScalarFunction(f) = e
        && f.name() == "contains"
        && f.args.len() == 2
        && matches!(&f.args[0], Expr::Column(c) if c.name=="line")
        && let Some(value) = text(&f.args[1])
    {
        lines.push(format!(" |= {}", quoted(value)));
    }
    if engine == "loki"
        && let Expr::Like(like) = e
    {
        if !like.negated
            && !like.case_insensitive
            && like.escape_char.is_none()
            && matches!(like.expr.as_ref(),Expr::Column(c) if c.name=="line")
            && let Some(pattern) = text(&like.pattern)
            && let Some(inside) = pattern.strip_prefix('%').and_then(|p| p.strip_suffix('%'))
            && !inside.contains(['%', '_', '\\'])
        {
            lines.push(format!(" |= {}", quoted(inside)));
        }
    }
    Ok(())
}
