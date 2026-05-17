//! Function-name registry for the Cypher runtime.
//!
//! The dispatch in [`super::eval_call`] and [`super::cypher_call`] is a
//! large `match (name, args)` table. Historically, alias groups like
//! `tofloat` / `to_float` / `float` reached the table separately and
//! could be split across reachable and unreachable arms, so adding a
//! function meant searching the table for every spelling.
//!
//! This module pre-resolves every known alias to a single canonical
//! name BEFORE dispatch. Downstream code only needs to match the
//! canonical spelling. Adding an alias is a one-line change here, and
//! it cannot accidentally bypass a newer implementation.
//!
//! Scope: this layer only normalizes the function NAME. It does not
//! own argument coercion or null propagation; those still live with
//! the implementation arms because they depend on argument types.

use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::OnceLock;

/// A canonical function name plus the aliases that resolve to it.
struct AliasGroup {
    canonical: &'static str,
    aliases: &'static [&'static str],
}

/// Source of truth for alias groups. Adding an entry here is enough to
/// make every spelling reach the canonical implementation arm.
///
/// IMPORTANT: cast-family entries (`to_int*`, `to_float*`, `to_string`,
/// `to_date`, etc.) are listed for predictable lookup, but their match
/// arms still delegate to the existing `casts.rs` helpers so this layer
/// stays out of the parallel cast-semantics refactor.
const ALIAS_GROUPS: &[AliasGroup] = &[
    // --- string casts ---
    //
    // Cypher's `toFloat` / `toInteger` / `toBoolean` / `toString` are
    // LENIENT (return null on conversion failure). Kuzu's `to_float`,
    // `to_double`, `float`, `double`, ... are STRICT (error on
    // conversion failure). The families share a namespace but have
    // intentionally different semantics, so each keeps its own
    // canonical entry. The registry guarantees every spelling reaches
    // exactly one arm; it does NOT collapse strict and lenient casts.
    AliasGroup {
        canonical: "to_float",
        aliases: &["float"],
    },
    AliasGroup {
        canonical: "to_double",
        aliases: &["todouble", "double"],
    },
    AliasGroup {
        canonical: "to_string",
        aliases: &["to_str", "str"],
    },
    // Cypher-style lenient casts keep their own canonical so we can
    // dispatch them to the older `cast_to_*` helpers.
    AliasGroup {
        canonical: "tofloat",
        aliases: &[],
    },
    AliasGroup {
        canonical: "tostring",
        aliases: &[],
    },
    AliasGroup {
        canonical: "tointeger",
        aliases: &[],
    },
    AliasGroup {
        canonical: "toboolean",
        aliases: &[],
    },
    AliasGroup {
        canonical: "to_int8",
        aliases: &["toint8", "int8"],
    },
    AliasGroup {
        canonical: "to_int16",
        aliases: &["toint16", "int16"],
    },
    AliasGroup {
        canonical: "to_int32",
        aliases: &["toint32", "int32"],
    },
    AliasGroup {
        canonical: "to_int64",
        aliases: &["toint64", "int64", "to_serial", "toserial", "serial"],
    },
    AliasGroup {
        canonical: "to_int128",
        aliases: &["toint128", "int128"],
    },
    AliasGroup {
        canonical: "to_uint8",
        aliases: &["touint8", "uint8"],
    },
    AliasGroup {
        canonical: "to_uint16",
        aliases: &["touint16", "uint16"],
    },
    AliasGroup {
        canonical: "to_uint32",
        aliases: &["touint32", "uint32"],
    },
    AliasGroup {
        canonical: "to_uint64",
        aliases: &["touint64", "uint64"],
    },
    AliasGroup {
        canonical: "to_uint128",
        aliases: &["touint128", "uint128"],
    },
    // --- string helpers ---
    AliasGroup {
        canonical: "lower",
        aliases: &["tolower", "lcase"],
    },
    AliasGroup {
        canonical: "upper",
        aliases: &["toupper", "ucase"],
    },
    // --- list helpers ---
    AliasGroup {
        canonical: "list_append",
        aliases: &["array_append", "array_push_back"],
    },
    AliasGroup {
        canonical: "list_prepend",
        aliases: &["array_prepend", "array_push_front"],
    },
    AliasGroup {
        canonical: "list_concat",
        aliases: &["list_cat", "array_concat", "array_cat"],
    },
    AliasGroup {
        canonical: "list_element",
        aliases: &["element_at"],
    },
    AliasGroup {
        canonical: "list_position",
        aliases: &["array_indexof", "array_position"],
    },
    AliasGroup {
        canonical: "list_contains",
        aliases: &["list_has", "array_contains", "array_has"],
    },
    // --- temporal accessors ---
    AliasGroup {
        canonical: "millisecond",
        aliases: &["ms"],
    },
    // --- graph element accessors ---
    AliasGroup {
        canonical: "start_node",
        aliases: &["startnode"],
    },
    AliasGroup {
        canonical: "end_node",
        aliases: &["endnode"],
    },
    AliasGroup {
        canonical: "relationships",
        aliases: &["rels"],
    },
    // --- nested / struct ---
    AliasGroup {
        canonical: "struct_extract",
        aliases: &["struct_extract_by_name", "map_extract_value"],
    },
];

fn table() -> &'static HashMap<&'static str, &'static str> {
    static TABLE: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();
    TABLE.get_or_init(|| {
        let mut map = HashMap::new();
        for group in ALIAS_GROUPS {
            // The canonical name resolves to itself so callers can blindly
            // look up any name without checking membership first.
            map.insert(group.canonical, group.canonical);
            for alias in group.aliases {
                debug_assert!(
                    !map.contains_key(*alias) || map[*alias] == group.canonical,
                    "alias `{alias}` is registered for multiple canonical names",
                );
                map.insert(*alias, group.canonical);
            }
        }
        map
    })
}

/// Return the canonical spelling for `name`, or `name` itself if no
/// alias is registered. The input is lowercased so callers do not have
/// to normalize casing themselves.
pub(crate) fn canonical_name(name: &str) -> Cow<'_, str> {
    let lower = name.to_ascii_lowercase();
    if let Some(canonical) = table().get(lower.as_str()) {
        return Cow::Borrowed(*canonical);
    }
    Cow::Owned(lower)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alias_groups_resolve_to_canonical() {
        for group in ALIAS_GROUPS {
            for spelling in std::iter::once(&group.canonical).chain(group.aliases.iter()) {
                let resolved = canonical_name(spelling);
                assert_eq!(
                    resolved.as_ref(),
                    group.canonical,
                    "alias `{spelling}` should resolve to `{}`",
                    group.canonical,
                );
                // Casing must not matter.
                let upper = spelling.to_ascii_uppercase();
                assert_eq!(canonical_name(&upper).as_ref(), group.canonical);
            }
        }
    }

    #[test]
    fn unknown_names_lowercase_passthrough() {
        assert_eq!(canonical_name("UnknownFn").as_ref(), "unknownfn");
        assert_eq!(canonical_name("already_lower").as_ref(), "already_lower");
    }

    #[test]
    fn no_alias_collisions() {
        // Building the table panics in debug mode on collisions; force it.
        let _ = table();
    }
}
