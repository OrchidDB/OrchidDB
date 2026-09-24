//! Strategy verification on grammar tokens, before lowering unsupported mutations.
use super::{GremlinParseError, GremlinToken, Result};

pub(super) fn verify(tokens: &[GremlinToken]) -> Result<()> {
    for query in tokens.split(|t| t.text == ";") {
        let mut read_only = false;
        for (i, token) in query.iter().enumerate() {
            if !matches!(token.text.as_str(), "withStrategies" | "withoutStrategies")
                || i == 0 || query[i - 1].text != "."
                || query.get(i + 1).is_none_or(|t| t.text != "(") {
                continue;
            }
            let mut depth = 1;
            for argument in &query[i + 2..] {
                match argument.text.as_str() {
                    "(" => depth += 1,
                    ")" => { depth -= 1; if depth == 0 { break; } },
                    "ReadOnlyStrategy" if depth == 1 => read_only = token.text == "withStrategies",
                    _ => (),
                }
            }
        }
        if read_only && query.windows(3).any(|w| {
            w[0].text == "." && w[2].text == "(" && matches!(w[1].text.as_str(),
                "addV" | "addE" | "mergeV" | "mergeE" | "property" | "drop" | "read")
        }) {
            return Err(GremlinParseError::Parse(
                "The provided traversal has a mutating step and thus is not read only".into()));
        }
    }
    Ok(())
}
