//! Shared Orchid v1 string-to-double contract. Adapter bindings must not copy it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParseError { Invalid, NonFinite }

/// Rust Unicode whitespace trimming, underscore removal, decimal parsing, and
/// finite results. Numeric inputs have their own conversion contract.
pub fn parse_string(input: &str) -> Result<f64, ParseError> {
    let trimmed = input.trim();
    let parsed = if trimmed.contains('_') {
        trimmed.replace('_', "").parse::<f64>()
    } else {
        trimmed.parse::<f64>()
    }.map_err(|_| ParseError::Invalid)?;
    if parsed.is_finite() { Ok(parsed) } else { Err(ParseError::NonFinite) }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn orchid_v1_boundaries() {
        for (input, expected) in [("1.25", Some(1.25)), ("\u{a0}__1__2__\u{3000}", Some(12.0)),
            (".5", Some(0.5)), ("1e-999", Some(0.0)), ("-0", Some(-0.0)),
            ("", None), ("foo", None), ("1 2", None), ("NaN", None), ("Infinity", None), ("1e999", None)] {
            assert_eq!(parse_string(input).ok().map(f64::to_bits), expected.map(f64::to_bits), "{input:?}");
        }
    }
}
