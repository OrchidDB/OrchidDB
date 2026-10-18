//! Compile-time clock classification shared by relational lowering.
//!
//! An explicit constructor argument is interpreted by `construct`; it does
//! not consult a clock even when the constructor has a clock qualifier.
//! This classification never evaluates or drops arguments.

/// Whether this temporal call can observe execution time. Argument purity and
/// arity/type validation remain the caller's responsibility.
pub(crate) fn may_read_clock(name: &str, arity: usize) -> bool {
    let Some(kind) = name.strip_prefix("cypher_temporal.") else {
        return false;
    };
    let (kind, qualified) = match kind.rsplit_once('.') {
        Some((kind, "transaction" | "statement" | "realtime")) => (kind, true),
        _ => (kind, false),
    };
    if matches!(
        kind,
        "date" | "localtime" | "time" | "localdatetime" | "datetime"
    ) {
        // Restrict the newly foldable qualified form to the runtime's exact
        // one-argument constructor arm. Invalid arities stay conservative.
        return arity == 0 || (qualified && arity != 1);
    }
    // Preserve the former conservative policy for unknown constructors.
    arity == 0 || qualified
}

#[cfg(test)]
mod tests {
    use super::may_read_clock;

    #[test]
    fn explicit_constructor_arguments_never_read_clocks() {
        for kind in ["date", "localtime", "time", "localdatetime", "datetime"] {
            for clock in ["transaction", "statement", "realtime"] {
                let name = format!("cypher_temporal.{kind}.{clock}");
                assert!(!may_read_clock(&name, 1), "{name}");
                assert!(may_read_clock(&name, 0), "{name}");
                assert!(may_read_clock(&name, 2), "{name}");
            }
            assert!(may_read_clock(&format!("cypher_temporal.{kind}"), 0));
            assert!(!may_read_clock(&format!("cypher_temporal.{kind}"), 1));
        }
        assert!(may_read_clock("cypher_temporal.unknown.realtime", 1));
        assert!(!may_read_clock("cypher_temporal.duration.inseconds", 2));
        assert!(!may_read_clock("size", 1));
    }
}
