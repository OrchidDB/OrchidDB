//! Exhaustive review of the enabled native catalog. A dependency upgrade that
//! adds a function must add an audit entry (enforced by the catalog test).
pub fn audit_note(name: &str, dialect: &str) -> &'static str {
    if dialect == "starrocks" { return "Only explicitly mapped StarRocks signatures are supported"; }
    let name = name.strip_prefix("fn.").unwrap_or(name);
    let pg = dialect == "postgres";
    match name {
        "arrow_cast" => {
            "Preparation lowers the Arrow type-name cast to a typed Cast expression; no runtime SQL UDF"
        }
        "arrow_typeof" | "arrow_metadata" => {
            "Preparation evaluates Arrow schema/type metadata; SQL values do not carry Arrow field metadata"
        }
        "version" => {
            "Preparation folds the native runtime version; database version() would describe a different engine"
        }
        "upper" | "lower" => {
            "Rust Unicode full case conversion, including expansions and contextual final sigma"
        }
        "initcap" => "Rust Unicode alphanumeric word boundaries and per-character case conversion",
        "translate" => {
            "Unicode 17 extended grapheme translation; first duplicate match and deletion preserved"
        }
        "lpad" | "rpad" => {
            "Unicode 17 grapheme truncation and code-point padding; literal Int32-range length"
        }
        "levenshtein" => {
            "Unicode code-point edit distance using a recursive SQL dynamic-programming row"
        }
        "overlay" => {
            "Character-based overlay; portable native wrapper fixes upstream UTF-8 slicing panic and boundary handling"
        }
        "chr" if pg => {
            "Literal non-NUL Unicode scalar after UInt32 wrapping; PostgreSQL text cannot contain NUL"
        }
        "chr" => "Unicode scalar conversion with native UInt32 wrapping",
        "power" if pg => {
            "Float64 literal exponents 0, 0.5, 1, 2 with exact IEEE boundaries; other powers stay native"
        }
        "power" => "Float64 IEEE power; integer/decimal overloads stay native",
        "sha224" | "sha384" | "sha512" if !pg => {
            "Core-SQL SHA-2 compression; binary/text input, multiple message blocks, no extension"
        }
        "sha224" | "sha256" | "sha384" | "sha512" | "md5" => {
            "Core hash implementation; native binary/text arguments"
        }
        "digest" => {
            "Literal md5/sha224/sha256/sha384/sha512; type-directed algorithm specialization"
        }
        "encode" => "Literal hex/base64/base64pad format; binary or UTF-8 text input",
        "decode" => {
            "Strict hex/base64/base64pad decoding with native padding and trailing-bit validation"
        }
        "to_timestamp_nanos" if pg => {
            "Microsecond-aligned literal epoch; arbitrary nanoseconds cannot be preserved by PostgreSQL timestamps"
        }
        "to_timestamp"
        | "to_timestamp_seconds"
        | "to_timestamp_millis"
        | "to_timestamp_micros"
        | "to_timestamp_nanos"
        | "from_unixtime" => {
            "Single Int64 epoch argument; text parsing and timezone overloads stay native; database timestamp range applies"
        }
        "to_date" | "to_local_time" => {
            "Single timezone-free date/timestamp argument; parsing/timezone conversion stays native"
        }
        "to_unixtime" => "Timezone-free timestamp of at most microsecond precision",
        "to_time" => "Time input of at most microsecond precision; text parsing stays native",
        "to_char" => "ISO date/time literal formats on timezone-free date/timestamp input",
        "make_date" => "Integer year/month/day construction within database date range",
        "make_time" => "Integer hour and literal minute/second in 0..59",
        "date_part" | "date_trunc" => {
            "Supported literal units on timezone-free timestamps of at most microsecond precision"
        }
        "date_bin" => {
            "Three arguments: positive fixed microsecond interval and timezone-free microsecond source/origin"
        }
        "current_date" | "current_time" | "now" => {
            "Stable calls are bound during query preparation"
        }
        "regexp_like" | "regexp_count" | "regexp_instr" | "regexp_match" | "regexp_replace" => {
            "Literal common Rust/RE2/POSIX subset; no captures, alternation, Unicode classes, lazy quantifiers, empty matches, or replacement backreferences"
        }
        "map" | "map_entries" | "map_extract" | "map_keys" | "map_values" => {
            "Typed maps; DuckDB native MAP and PostgreSQL ordered JSONB entries with exact scalar leaves"
        }
        "struct" | "named_struct" | "get_field" | "arrays_zip" => {
            "Type-directed field names and nested values; native DuckDB records and schema-directed PostgreSQL JSONB"
        }
        "union_extract" | "union_tag" => {
            "Typed union variants, including null payloads and noncontiguous Arrow type IDs"
        }
        "make_array" => "One or more scalar boolean, signed-integer, or text arguments",
        "array_length" | "array_ndims" | "array_dims" | "array_any_value" | "array_append"
        | "array_prepend" | "array_element" | "array_distinct" | "array_except" | "array_has"
        | "array_has_all" | "array_has_any" | "array_intersect" | "array_pop_back"
        | "array_pop_front" | "array_positions" | "array_remove" | "array_remove_all"
        | "array_replace" | "array_replace_all" | "array_reverse" | "array_union"
        | "cardinality" | "empty" | "flatten" => {
            "Flat boolean, signed-integer, or text lists; nulls, duplicates, and native order preserved"
        }
        "array_distance" => {
            "Flat signed-integer lists; null elements propagate; unequal lengths fail"
        }
        "array_min" | "array_max" => {
            "Flat lists; PostgreSQL text ordering stays native because it depends on collation"
        }
        "array_concat" => "Two flat lists; null lists are treated as empty",
        "array_position" => "Flat list; optional positive literal start position",
        "array_repeat" | "array_resize" => "Flat/scalar values with nonnegative literal size",
        "array_remove_n" | "array_replace_n" => {
            "Flat lists with a non-null literal count; native zero/negative count semantics preserved"
        }
        "array_slice" => "Flat lists; three arguments or a positive literal stride",
        "array_sort" => {
            "Flat lists with literal ASC/DESC and NULLS FIRST/LAST; PostgreSQL text collation stays native"
        }
        "array_to_string" => "Text lists only; numeric/boolean formatting differs between engines",
        "range" | "generate_series" => {
            "Int64 bounds and optional nonzero literal step; endpoint inclusion preserved"
        }
        "string_to_array" => {
            "Two/three arguments; empty text, null delimiter, and null replacement preserved"
        }
        "left" | "right" | "repeat" | "substr" => {
            "Literal Int32-range position/count; substring length must be nonnegative"
        }
        "split_part" => "Nonzero literal Int32-range field index",
        "substr_index" => "Native left/right occurrence counting, including overlapping delimiters",
        "round" | "trunc" if pg => {
            "Float64 single-argument overload; binary rescaling precision overload stays native"
        }
        "round" | "trunc" => "Float64 and optional literal precision in -15..15",
        "exp" | "cosh" | "sinh" | "cot" => {
            "Float64 with native special-value/domain/overflow handling"
        }
        "abs" | "acos" | "acosh" | "asin" | "asinh" | "atan" | "atan2" | "atanh" | "cbrt"
        | "ceil" | "cos" | "degrees" | "factorial" | "floor" | "gcd" | "isnan" | "iszero"
        | "lcm" | "ln" | "log" | "log10" | "log2" | "nanvl" | "pi" | "radians" | "signum"
        | "sin" | "sqrt" | "tan" | "tanh" => {
            "Numeric mapping with native result width and domain guards"
        }
        "ascii" | "bit_length" | "btrim" | "character_length" | "contains" | "ends_with"
        | "find_in_set" | "ltrim" | "octet_length" | "replace" | "reverse" | "rtrim"
        | "starts_with" | "strpos" | "to_hex" => {
            "String/code-point mapping; native argument/result types retained"
        }
        "coalesce" | "concat" | "concat_ws" | "greatest" | "least" | "nullif" | "nvl" | "nvl2" => {
            "Variadic/null semantics preserved; backend value/type representability still applies"
        }
        "random" | "uuid" => "Volatile backend implementation; each argument evaluated once",
        _ => "NOT AUDITED: newly enabled native function",
    }
}
