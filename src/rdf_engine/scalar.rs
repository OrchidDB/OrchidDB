//! SPARQL scalar contracts that differ from DuckDB builtins.
//! SQL IR calls these typed functions; failures return an unbound RDF value.
use arrow::array::{Array, RecordBatch, StringArray};
use arrow::datatypes::DataType;
use duckdb::vscalar::{ArrowFunctionSignature, VArrowScalar};
use regex::{Regex, RegexBuilder};
use sha2::{Digest, Sha384, Sha512};
use std::sync::Arc;

pub(super) struct SparqlScalar;

impl VArrowScalar for SparqlScalar {
    type State = ();
    fn signatures() -> Vec<ArrowFunctionSignature> {
        vec![ArrowFunctionSignature::exact(
            vec![DataType::Utf8; 5],
            DataType::Utf8,
        )]
    }
    fn invoke(_: &(), input: RecordBatch) -> Result<Arc<dyn Array>, Box<dyn std::error::Error>> {
        let columns = input
            .columns()
            .iter()
            .map(|column| {
                column
                    .as_any()
                    .downcast_ref::<StringArray>()
                    .ok_or("SPARQL scalar expects strings")
            })
            .collect::<Result<Vec<_>, _>>()?;
        let values = (0..input.num_rows())
            .map(|row| {
                if columns.iter().any(|column| column.is_null(row)) {
                    return None;
                }
                evaluate(
                    columns[0].value(row),
                    columns[1].value(row),
                    columns[2].value(row),
                    columns[3].value(row),
                    columns[4].value(row),
                )
            })
            .collect::<Vec<_>>();
        Ok(Arc::new(StringArray::from(values)))
    }
}

fn pattern(source: &str, flags: &str) -> Option<Regex> {
    if flags.chars().any(|flag| !"imsxq".contains(flag)) {
        return None;
    }
    let source = if flags.contains('q') {
        regex::escape(source)
    } else if flags.contains('x') {
        // XPath strips whitespace outside character classes, preserving escapes.
        let mut result = String::new();
        let mut class = false;
        let mut escaped = false;
        for ch in source.chars() {
            if escaped {
                result.push(ch);
                escaped = false;
                continue;
            }
            if ch == '\\' {
                escaped = true;
                result.push(ch);
                continue;
            }
            if ch == '[' {
                class = true;
            } else if ch == ']' {
                class = false;
            }
            if class || !matches!(ch, ' ' | '\t' | '\r' | '\n') {
                result.push(ch);
            }
        }
        result
    } else {
        source.into()
    };
    RegexBuilder::new(&source)
        .case_insensitive(flags.contains('i'))
        .multi_line(flags.contains('m'))
        .dot_matches_new_line(flags.contains('s'))
        .build()
        .ok()
}

fn replacement(source: &str) -> Option<String> {
    let mut out = String::new();
    let mut chars = source.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '\\' => match chars.next()? {
                '$' => out.push_str("$$"),
                '\\' => out.push('\\'),
                _ => return None,
            },
            '$' => {
                let digit = chars.next()?;
                if !digit.is_ascii_digit() {
                    return None;
                }
                out.push_str("${");
                out.push(digit);
                out.push('}');
            }
            _ => out.push(ch),
        }
    }
    Some(out)
}

fn evaluate(op: &str, text: &str, argument: &str, substitute: &str, flags: &str) -> Option<String> {
    Some(match op {
        "resolve_iri" => {
            if argument.is_empty() {
                oxiri::Iri::parse(text).ok()?.to_string()
            } else {
                oxiri::Iri::parse(argument).ok()?.resolve(text).ok()?.to_string()
            }
        }
        "compare_datetime" => compare_values(substitute,
            text.parse::<oxsdatatypes::DateTime>().ok()?, argument.parse().ok()?)?,
        "compare_date" => compare_values(substitute,
            text.parse::<oxsdatatypes::Date>().ok()?, argument.parse().ok()?)?,
        "decimal_divide" => {
            use num_traits::Zero;
            let left: bigdecimal::BigDecimal = text.parse().ok()?;
            let right: bigdecimal::BigDecimal = argument.parse().ok()?;
            if right.is_zero() { return None; }
            let result = (left / right).normalized().to_plain_string();
            if result.contains('.') { result } else { format!("{result}.0") }
        }
        "cast_string" => match argument.strip_prefix("http://www.w3.org/2001/XMLSchema#") {
            Some("boolean") => text.parse::<oxsdatatypes::Boolean>().ok()?.to_string(),
            Some("decimal") => text.parse::<oxsdatatypes::Decimal>().ok()?.to_string(),
            Some("double") => text.parse::<oxsdatatypes::Double>().ok()?.to_string(),
            Some("float") => text.parse::<oxsdatatypes::Float>().ok()?.to_string(),
            Some("integer") => text.parse::<num_bigint::BigInt>().ok()?.to_string(),
            _ => text.to_owned(),
        },
        "sha384" => format!("{:x}", Sha384::digest(text.as_bytes())),
        "sha512" => format!("{:x}", Sha512::digest(text.as_bytes())),
        "encode_for_uri" => {
            let mut out = String::new();
            for byte in text.bytes() {
                if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) {
                    out.push(byte as char);
                } else {
                    use std::fmt::Write;
                    write!(out, "%{byte:02X}").ok()?;
                }
            }
            out
        }
        "regex" => pattern(argument, flags)?.is_match(text).to_string(),
        "replace" => {
            let regex = pattern(argument, flags)?;
            if regex.is_match("") {
                return None;
            }
            regex
                .replace_all(text, replacement(substitute)?)
                .into_owned()
        }
        "timezone" => {
            let parsed = chrono::DateTime::parse_from_rfc3339(text).ok()?;
            let offset = parsed.offset().local_minus_utc();
            let hours = offset.abs() / 3600;
            let minutes = (offset.abs() % 3600) / 60;
            let mut out = if offset < 0 { "-PT" } else { "PT" }.to_string();
            if hours != 0 {
                out.push_str(&format!("{hours}H"));
            }
            if minutes != 0 {
                out.push_str(&format!("{minutes}M"));
            }
            if offset == 0 {
                out.push_str("0S");
            }
            out
        }
        _ => return None,
    })
}

fn compare_values<T: PartialOrd>(op: &str, left: T, right: T) -> Option<String> {
    Some(match op {
        "eq" => left == right,
        "ne" => left != right,
        op => {
            let order = left.partial_cmp(&right)?;
            match op {
                "lt" => order.is_lt(), "le" => !order.is_gt(),
                "gt" => order.is_gt(), "ge" => !order.is_lt(),
                _ => return None,
            }
        }
    }.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn xpath_flags_replacements_and_errors() {
        assert_eq!(
            evaluate("regex", "a\nb", "^b$", "", "m").as_deref(),
            Some("true")
        );
        assert_eq!(
            evaluate("regex", "A.B", "a.b", "", "qi").as_deref(),
            Some("true")
        );
        assert_eq!(
            evaluate("regex", "a b", "a [ ] b", "", "x").as_deref(),
            Some("true")
        );
        assert_eq!(
            evaluate("replace", "abcd", "(ab)", "[$1=]", "").as_deref(),
            Some("[ab=]cd")
        );
        assert!(evaluate("regex", "x", "[", "", "").is_none());
        assert_eq!(
            evaluate("encode_for_uri", "a /é", "", "", "").as_deref(),
            Some("a%20%2F%C3%A9")
        );
        assert_eq!(
            evaluate("timezone", "2020-01-01T00:00:00-05:30", "", "", "").as_deref(),
            Some("-PT5H30M")
        );
    }
}
