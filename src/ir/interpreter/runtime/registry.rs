//! Function-name registry for the Cypher runtime.
//!
//! The dispatch in [`super::eval_call`] and [`super::cypher::cypher_call`] is a
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
    AliasGroup {
        canonical: "uuid",
        aliases: &["to_uuid", "touuid"],
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
    AliasGroup {
        canonical: "contains",
        aliases: &["contains_fn"],
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

/// Explicit interpreter operator inventory. Unknown engine functions must never
/// reach the interpreter's generic null-propagation fallback during folding.
/// Keep this list in sync when adding a dispatch arm; aliases resolve first.
pub(crate) fn is_known_function(name: &str) -> bool {
    let canonical = canonical_name(name);
    KNOWN_FUNCTIONS.binary_search(&canonical.as_ref()).is_ok()
}

const KNOWN_FUNCTIONS: &[&str] = &[
    "abs",
    "acos",
    "add10",
    "add4",
    "add5",
    "add7",
    "add8",
    "add_with_default",
    "adddefault",
    "adddefault1",
    "addwithdefault",
    "all_property_values",
    "any_property",
    "appendelement",
    "array_concat",
    "array_contains",
    "array_cosine_similarity",
    "array_cross_product",
    "array_distance",
    "array_dot_product",
    "array_extract",
    "array_indexof",
    "array_inner_product",
    "array_length",
    "array_position",
    "array_slice",
    "array_squared_distance",
    "array_value",
    "asin",
    "atan",
    "atan2",
    "bitshift_left",
    "bitshift_right",
    "bitwise_and",
    "bitwise_or",
    "blob",
    "bulk_set",
    "cardinality",
    "case_macro",
    "cast",
    "cast_bigdecimal",
    "cast_bigint",
    "cast_bool",
    "cast_byte",
    "cast_date",
    "cast_double",
    "cast_float",
    "cast_int",
    "cast_long",
    "cast_number",
    "cast_short",
    "cast_string",
    "cbrt",
    "ceil",
    "century",
    "coalesce",
    "coin_keep",
    "concat",
    "concat_ws",
    "conjoin",
    "constant_or_null",
    "contains",
    "cos",
    "cot",
    "cypher_eq",
    "cypher_gt",
    "cypher_gte",
    "cypher_in",
    "cypher_label",
    "cypher_lt",
    "cypher_lte",
    "cypher_neq",
    "cypher_properties_match",
    "cypher_property_star",
    "cypher_slice",
    "cypher_star",
    "cypher_subscript",
    "date",
    "date_add",
    "date_diff",
    "date_part",
    "date_trunc",
    "datetime",
    "datetime_literal",
    "dayname",
    "decode",
    "degrees",
    "dot_product",
    "duration",
    "e",
    "edge_both",
    "edge_dst",
    "edge_src",
    "element_at",
    "element_kind",
    "element_map",
    "encode",
    "end_node",
    "ends_with",
    "endswith",
    "epoch_ms",
    "epochmillis",
    "error",
    "even",
    "exists",
    "exp",
    "factorial",
    "floating_literal",
    "floor",
    "fold_reduce",
    "format_concat",
    "format_placeholder",
    "func_macro",
    "gamma",
    "gen_random_uuid",
    "graph_algorithm",
    "greatest",
    "gremlin_cast_date",
    "gremlin_cast_int",
    "gremlin_cast_string",
    "gremlin_dedup_key",
    "gremlin_id",
    "gremlin_id_token",
    "gremlin_math_bin",
    "gremlin_order_key",
    "gremlin_scan_order",
    "gremlin_split_ws",
    "gremlin_string_concat",
    "gremlin_string_conjoin",
    "gremlin_string_lcase",
    "gremlin_string_length",
    "gremlin_string_local_concat",
    "gremlin_string_local_conjoin",
    "gremlin_string_local_lcase",
    "gremlin_string_local_length",
    "gremlin_string_local_ltrim",
    "gremlin_string_local_replace",
    "gremlin_string_local_reverse",
    "gremlin_string_local_rtrim",
    "gremlin_string_local_split",
    "gremlin_string_local_split_ws",
    "gremlin_string_local_substring",
    "gremlin_string_local_trim",
    "gremlin_string_local_ucase",
    "gremlin_string_ltrim",
    "gremlin_string_replace",
    "gremlin_string_reverse",
    "gremlin_string_rtrim",
    "gremlin_string_split",
    "gremlin_string_split_ws",
    "gremlin_string_substring",
    "gremlin_string_trim",
    "gremlin_string_ucase",
    "gremlin_substring",
    "gremlin_sum_result",
    "gremlin_traversal_list_combine",
    "gremlin_traversal_list_difference",
    "gremlin_traversal_list_disjunct",
    "gremlin_traversal_list_intersect",
    "gremlin_traversal_list_merge",
    "gremlin_traversal_list_product",
    "gremlin_unfold_items",
    "gremlin_within",
    "hash",
    "haversin",
    "head",
    "id",
    "ifnull",
    "in",
    "index_list",
    "index_map",
    "initcap",
    "int_literal",
    "integer_literal",
    "internal_id",
    "interval",
    "interval_literal",
    "is_acyclic",
    "is_trail",
    "isempty",
    "keys",
    "label",
    "labels",
    "last",
    "last_day",
    "lcase",
    "least",
    "left",
    "len",
    "length",
    "levenshtein",
    "lgamma",
    "list_any_value",
    "list_append",
    "list_at",
    "list_avg",
    "list_combine",
    "list_concat",
    "list_contains",
    "list_count",
    "list_creation",
    "list_difference",
    "list_disjunct",
    "list_distinct",
    "list_element",
    "list_extract",
    "list_has_all",
    "list_indexof",
    "list_intersect",
    "list_join",
    "list_length",
    "list_literal",
    "list_max",
    "list_merge",
    "list_min",
    "list_position",
    "list_prepend",
    "list_product",
    "list_restore_null_sentinels",
    "list_reverse",
    "list_reverse_sort",
    "list_size",
    "list_slice",
    "list_sort",
    "list_sum",
    "list_to_string",
    "list_transform",
    "list_unique",
    "ln",
    "local_cast_bigdecimal",
    "local_cast_bigint",
    "local_cast_bool",
    "local_cast_byte",
    "local_cast_date",
    "local_cast_double",
    "local_cast_float",
    "local_cast_int",
    "local_cast_long",
    "local_cast_number",
    "local_cast_short",
    "local_cast_string",
    "local_concat",
    "local_conjoin",
    "local_count",
    "local_dedup",
    "local_lcase",
    "local_length",
    "local_limit",
    "local_ltrim",
    "local_max",
    "local_mean",
    "local_min",
    "local_order",
    "local_order_by_key",
    "local_order_merge_map",
    "local_range",
    "local_replace",
    "local_reverse_strings",
    "local_rtrim",
    "local_skip",
    "local_split",
    "local_substring",
    "local_sum",
    "local_tail",
    "local_trim",
    "local_ucase",
    "log",
    "log10",
    "log2",
    "lower",
    "lpad",
    "ltrim",
    "make_date",
    "make_map",
    "make_project_map",
    "map",
    "map_extract",
    "map_get_display",
    "map_has_key",
    "map_keys",
    "map_literal",
    "map_values",
    "md5",
    "mod",
    "monthname",
    "multiply",
    "negate",
    "nestedscalarmacro",
    "nodes",
    "null_to_sentinel",
    "nullif",
    "octet_length",
    "odd",
    "parameter",
    "path_append",
    "path_append_after",
    "path_by_keys",
    "path_by_keys_keep_nulls",
    "path_from",
    "path_intermediate_pairs",
    "path_last_label_eq",
    "path_last_property_eq",
    "path_or_self",
    "path_pairs",
    "path_project_edges",
    "path_to",
    "pi",
    "pow",
    "prefix",
    "procedure_call",
    "prop_macro",
    "properties",
    "properties_list",
    "property",
    "property_element",
    "property_key",
    "property_map",
    "property_value",
    "radians",
    "rand",
    "random",
    "range",
    "recursive_relationship_path",
    "regex_match",
    "regexp_extract",
    "regexp_extract_all",
    "regexp_full_match",
    "regexp_matches",
    "regexp_replace",
    "regexp_split_to_array",
    "relationships",
    "repeat",
    "replace",
    "requested_property_values",
    "returnconstant",
    "reverse",
    "right",
    "round",
    "rowid",
    "rpad",
    "rtrim",
    "sack_apply",
    "scalarcase",
    "select_history_append",
    "select_key_or_binding",
    "select_key_or_binding_pop",
    "set_compact",
    "set_literal",
    "sha1",
    "sha256",
    "sign",
    "sin",
    "size",
    "split",
    "split_part",
    "sqrt",
    "start_node",
    "starts_with",
    "startswith",
    "str_literal",
    "str_split",
    "string",
    "string_split",
    "string_to_array",
    "struct_extract",
    "struct_pack",
    "substr",
    "substring",
    "suffix",
    "tail",
    "tan",
    "timestamp",
    "tinker_degree_centrality",
    "tinker_search",
    "to_blob",
    "to_bool",
    "to_date",
    "to_days",
    "to_double",
    "to_epoch_ms",
    "to_float",
    "to_hours",
    "to_int128",
    "to_int16",
    "to_int32",
    "to_int64",
    "to_int8",
    "to_microseconds",
    "to_milliseconds",
    "to_minutes",
    "to_months",
    "to_seconds",
    "to_string",
    "to_timestamp",
    "to_uint128",
    "to_uint16",
    "to_uint32",
    "to_uint64",
    "to_uint8",
    "to_years",
    "tobool",
    "toboolean",
    "tofloat",
    "tointeger",
    "tostring",
    "tree_value",
    "trim",
    "type",
    "typeof",
    "typeof_matches",
    "ucase",
    "union_extract",
    "union_extract_by_tag",
    "union_tag",
    "union_value",
    "upper",
    "uuid",
    "value_map",
    "value_map_tokens",
    "var_macro",
    "xor",
];

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
    fn known_inventory_is_sorted_unique_and_aliases_resolve() {
        assert!(KNOWN_FUNCTIONS.windows(2).all(|pair| pair[0] < pair[1]));
        for name in ["abs", "tolower", "toupper", "array_append", "toint64"] {
            assert!(is_known_function(name), "{name}");
        }
        // Alias normalization does not imply a runtime implementation.
        assert!(!is_known_function("millisecond"));
        assert!(!is_known_function("new_engine_udf"));
    }

    #[test]
    fn no_alias_collisions() {
        // Building the table panics in debug mode on collisions; force it.
        let _ = table();
    }
}
