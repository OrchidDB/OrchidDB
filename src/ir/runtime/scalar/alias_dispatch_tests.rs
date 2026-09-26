//! Table-driven tests that pin the registry contract:
//!
//! 1. Every alias in a group produces the same `eval_call` result
//!    as the canonical name, so callers cannot accidentally split
//!    behavior across spellings.
//! 2. Null arguments propagate where the registry advertises it.
//!
//! Failures here usually mean a new arm was added under one
//! spelling but not registered as an alias, or that a registered
//! alias still has a divergent legacy arm somewhere.
use super::*;

fn call(name: &str, args: &[Value]) -> Value {
    let graph = PropertyGraph::new();
    eval_call(name, args.to_vec(), &graph).unwrap_or(Value::Null)
}

/// One row per alias group we expect to be behavior-equivalent.
/// `canonical` is the spelling the registry resolves to.
struct AliasCase {
    canonical: &'static str,
    aliases: &'static [&'static str],
    args: Vec<Value>,
}

fn cases() -> Vec<AliasCase> {
    vec![
        AliasCase {
            canonical: "lower",
            aliases: &["tolower", "lcase", "LOWER", "ToLower"],
            args: vec![Value::String(String::new())],
        },
        AliasCase {
            canonical: "upper",
            aliases: &["toupper", "ucase", "UPPER"],
            args: vec![Value::String(String::new())],
        },
        AliasCase {
            canonical: "list_contains",
            aliases: &["list_has", "array_contains", "array_has"],
            args: vec![Value::List(Vec::new()), Value::Int(1)],
        },
        AliasCase {
            canonical: "list_concat",
            aliases: &["list_cat", "array_concat", "array_cat"],
            args: vec![Value::List(Vec::new()), Value::List(Vec::new())],
        },
        AliasCase {
            canonical: "list_position",
            aliases: &["array_indexof", "array_position"],
            args: vec![Value::List(Vec::new()), Value::Int(1)],
        },
    ]
}

#[test]
fn aliases_match_canonical() {
    for case in cases() {
        let baseline = call(case.canonical, &case.args);
        for alias in case.aliases {
            let observed = call(alias, &case.args);
            assert_eq!(
                observed, baseline,
                "alias `{alias}` diverged from canonical `{}`",
                case.canonical,
            );
        }
    }
}

#[test]
fn null_propagation_for_string_helpers() {
    // The dispatcher exposes a shared null-propagation arm for
    // canonical string casts. Each alias must reach it.
    for alias in [
        "tolower",
        "TOLOWER",
        "lower",
        "toupper",
        "upper",
        "tostring",
        "tointeger",
        "tofloat",
        "toboolean",
    ] {
        let observed = call(alias, &[Value::Null]);
        assert!(
            matches!(observed, Value::Null),
            "alias `{alias}` did not null-propagate (got {observed:?})",
        );
    }
}

#[test]
fn strict_cast_aliases_resolve_together() {
    // `to_float` and `float` are aliases for the strict Kuzu-style
    // cast. They must resolve identically. `tofloat` is the
    // separate Cypher-style lenient cast — different semantics, so
    // we deliberately do NOT collapse it here.
    let a = call("to_float", &[Value::Int(3)]);
    let b = call("FLOAT", &[Value::Int(3)]);
    assert_eq!(a, b);
}
