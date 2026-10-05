| Function | DuckDB SQL | PostgreSQL SQL | DuckDB scope / reason | PostgreSQL scope / reason |
| --- | --- | --- | --- | --- |
| `fn.abs` | mapped* | mapped* | Numeric mapping with native result width and domain guards | Numeric mapping with native result width and domain guards |
| `fn.acos` | mapped* | mapped* | Numeric mapping with native result width and domain guards | Numeric mapping with native result width and domain guards |
| `fn.acosh` | mapped* | mapped* | Numeric mapping with native result width and domain guards | Numeric mapping with native result width and domain guards |
| `fn.array_any_value` | mapped* | mapped* | Flat boolean, signed-integer, or text lists; nulls, duplicates, and native order preserved | Flat boolean, signed-integer, or text lists; nulls, duplicates, and native order preserved |
| `fn.array_append` | mapped* | mapped* | Flat boolean, signed-integer, or text lists; nulls, duplicates, and native order preserved | Flat boolean, signed-integer, or text lists; nulls, duplicates, and native order preserved |
| `fn.array_concat` | mapped* | mapped* | Two flat lists; null lists are treated as empty | Two flat lists; null lists are treated as empty |
| `fn.array_dims` | mapped* | mapped* | Flat boolean, signed-integer, or text lists; nulls, duplicates, and native order preserved | Flat boolean, signed-integer, or text lists; nulls, duplicates, and native order preserved |
| `fn.array_distance` | mapped* | mapped* | Flat signed-integer lists; null elements propagate; unequal lengths fail | Flat signed-integer lists; null elements propagate; unequal lengths fail |
| `fn.array_distinct` | mapped* | mapped* | Flat boolean, signed-integer, or text lists; nulls, duplicates, and native order preserved | Flat boolean, signed-integer, or text lists; nulls, duplicates, and native order preserved |
| `fn.array_element` | mapped* | mapped* | Flat boolean, signed-integer, or text lists; nulls, duplicates, and native order preserved | Flat boolean, signed-integer, or text lists; nulls, duplicates, and native order preserved |
| `fn.array_except` | mapped* | mapped* | Flat boolean, signed-integer, or text lists; nulls, duplicates, and native order preserved | Flat boolean, signed-integer, or text lists; nulls, duplicates, and native order preserved |
| `fn.array_has` | mapped* | mapped* | Flat boolean, signed-integer, or text lists; nulls, duplicates, and native order preserved | Flat boolean, signed-integer, or text lists; nulls, duplicates, and native order preserved |
| `fn.array_has_all` | mapped* | mapped* | Flat boolean, signed-integer, or text lists; nulls, duplicates, and native order preserved | Flat boolean, signed-integer, or text lists; nulls, duplicates, and native order preserved |
| `fn.array_has_any` | mapped* | mapped* | Flat boolean, signed-integer, or text lists; nulls, duplicates, and native order preserved | Flat boolean, signed-integer, or text lists; nulls, duplicates, and native order preserved |
| `fn.array_intersect` | mapped* | mapped* | Flat boolean, signed-integer, or text lists; nulls, duplicates, and native order preserved | Flat boolean, signed-integer, or text lists; nulls, duplicates, and native order preserved |
| `fn.array_length` | mapped* | mapped* | Flat boolean, signed-integer, or text lists; nulls, duplicates, and native order preserved | Flat boolean, signed-integer, or text lists; nulls, duplicates, and native order preserved |
| `fn.array_max` | mapped* | mapped* | Flat lists; PostgreSQL text ordering stays native because it depends on collation | Flat lists; PostgreSQL text ordering stays native because it depends on collation |
| `fn.array_min` | mapped* | mapped* | Flat lists; PostgreSQL text ordering stays native because it depends on collation | Flat lists; PostgreSQL text ordering stays native because it depends on collation |
| `fn.array_ndims` | mapped* | mapped* | Flat boolean, signed-integer, or text lists; nulls, duplicates, and native order preserved | Flat boolean, signed-integer, or text lists; nulls, duplicates, and native order preserved |
| `fn.array_pop_back` | mapped* | mapped* | Flat boolean, signed-integer, or text lists; nulls, duplicates, and native order preserved | Flat boolean, signed-integer, or text lists; nulls, duplicates, and native order preserved |
| `fn.array_pop_front` | mapped* | mapped* | Flat boolean, signed-integer, or text lists; nulls, duplicates, and native order preserved | Flat boolean, signed-integer, or text lists; nulls, duplicates, and native order preserved |
| `fn.array_position` | mapped* | mapped* | Flat list; optional positive literal start position | Flat list; optional positive literal start position |
| `fn.array_positions` | mapped* | mapped* | Flat boolean, signed-integer, or text lists; nulls, duplicates, and native order preserved | Flat boolean, signed-integer, or text lists; nulls, duplicates, and native order preserved |
| `fn.array_prepend` | mapped* | mapped* | Flat boolean, signed-integer, or text lists; nulls, duplicates, and native order preserved | Flat boolean, signed-integer, or text lists; nulls, duplicates, and native order preserved |
| `fn.array_remove` | mapped* | mapped* | Flat boolean, signed-integer, or text lists; nulls, duplicates, and native order preserved | Flat boolean, signed-integer, or text lists; nulls, duplicates, and native order preserved |
| `fn.array_remove_all` | mapped* | mapped* | Flat boolean, signed-integer, or text lists; nulls, duplicates, and native order preserved | Flat boolean, signed-integer, or text lists; nulls, duplicates, and native order preserved |
| `fn.array_remove_n` | mapped* | mapped* | Flat lists with a non-null literal count; native zero/negative count semantics preserved | Flat lists with a non-null literal count; native zero/negative count semantics preserved |
| `fn.array_repeat` | mapped* | mapped* | Flat/scalar values with nonnegative literal size | Flat/scalar values with nonnegative literal size |
| `fn.array_replace` | mapped* | mapped* | Flat boolean, signed-integer, or text lists; nulls, duplicates, and native order preserved | Flat boolean, signed-integer, or text lists; nulls, duplicates, and native order preserved |
| `fn.array_replace_all` | mapped* | mapped* | Flat boolean, signed-integer, or text lists; nulls, duplicates, and native order preserved | Flat boolean, signed-integer, or text lists; nulls, duplicates, and native order preserved |
| `fn.array_replace_n` | mapped* | mapped* | Flat lists with a non-null literal count; native zero/negative count semantics preserved | Flat lists with a non-null literal count; native zero/negative count semantics preserved |
| `fn.array_resize` | mapped* | mapped* | Flat/scalar values with nonnegative literal size | Flat/scalar values with nonnegative literal size |
| `fn.array_reverse` | mapped* | mapped* | Flat boolean, signed-integer, or text lists; nulls, duplicates, and native order preserved | Flat boolean, signed-integer, or text lists; nulls, duplicates, and native order preserved |
| `fn.array_slice` | mapped* | mapped* | Flat lists; three arguments or a positive literal stride | Flat lists; three arguments or a positive literal stride |
| `fn.array_sort` | mapped* | mapped* | Flat lists with literal ASC/DESC and NULLS FIRST/LAST; PostgreSQL text collation stays native | Flat lists with literal ASC/DESC and NULLS FIRST/LAST; PostgreSQL text collation stays native |
| `fn.array_to_string` | mapped* | mapped* | Text lists only; numeric/boolean formatting differs between engines | Text lists only; numeric/boolean formatting differs between engines |
| `fn.array_union` | mapped* | mapped* | Flat boolean, signed-integer, or text lists; nulls, duplicates, and native order preserved | Flat boolean, signed-integer, or text lists; nulls, duplicates, and native order preserved |
| `fn.arrays_zip` | mapped* | mapped* | Type-directed field names and nested values; native DuckDB records and schema-directed PostgreSQL JSONB | Type-directed field names and nested values; native DuckDB records and schema-directed PostgreSQL JSONB |
| `fn.arrow_cast` | lowered* | lowered* | Preparation lowers the Arrow type-name cast to a typed Cast expression; no runtime SQL UDF | Preparation lowers the Arrow type-name cast to a typed Cast expression; no runtime SQL UDF |
| `fn.arrow_metadata` | lowered* | lowered* | Preparation evaluates Arrow schema/type metadata; SQL values do not carry Arrow field metadata | Preparation evaluates Arrow schema/type metadata; SQL values do not carry Arrow field metadata |
| `fn.arrow_typeof` | lowered* | lowered* | Preparation evaluates Arrow schema/type metadata; SQL values do not carry Arrow field metadata | Preparation evaluates Arrow schema/type metadata; SQL values do not carry Arrow field metadata |
| `fn.ascii` | mapped* | mapped* | String/code-point mapping; native argument/result types retained | String/code-point mapping; native argument/result types retained |
| `fn.asin` | mapped* | mapped* | Numeric mapping with native result width and domain guards | Numeric mapping with native result width and domain guards |
| `fn.asinh` | mapped* | mapped* | Numeric mapping with native result width and domain guards | Numeric mapping with native result width and domain guards |
| `fn.atan` | mapped* | mapped* | Numeric mapping with native result width and domain guards | Numeric mapping with native result width and domain guards |
| `fn.atan2` | mapped* | mapped* | Numeric mapping with native result width and domain guards | Numeric mapping with native result width and domain guards |
| `fn.atanh` | mapped* | mapped* | Numeric mapping with native result width and domain guards | Numeric mapping with native result width and domain guards |
| `fn.bit_length` | mapped* | mapped* | String/code-point mapping; native argument/result types retained | String/code-point mapping; native argument/result types retained |
| `fn.btrim` | mapped* | mapped* | String/code-point mapping; native argument/result types retained | String/code-point mapping; native argument/result types retained |
| `fn.cardinality` | mapped* | mapped* | Flat boolean, signed-integer, or text lists; nulls, duplicates, and native order preserved | Flat boolean, signed-integer, or text lists; nulls, duplicates, and native order preserved |
| `fn.cbrt` | mapped* | mapped* | Numeric mapping with native result width and domain guards | Numeric mapping with native result width and domain guards |
| `fn.ceil` | mapped* | mapped* | Numeric mapping with native result width and domain guards | Numeric mapping with native result width and domain guards |
| `fn.character_length` | mapped* | mapped* | String/code-point mapping; native argument/result types retained | String/code-point mapping; native argument/result types retained |
| `fn.chr` | mapped* | mapped* | Unicode scalar conversion with native UInt32 wrapping | Literal non-NUL Unicode scalar after UInt32 wrapping; PostgreSQL text cannot contain NUL |
| `fn.coalesce` | mapped* | mapped* | Variadic/null semantics preserved; backend value/type representability still applies | Variadic/null semantics preserved; backend value/type representability still applies |
| `fn.concat` | mapped* | mapped* | Variadic/null semantics preserved; backend value/type representability still applies | Variadic/null semantics preserved; backend value/type representability still applies |
| `fn.concat_ws` | mapped* | mapped* | Variadic/null semantics preserved; backend value/type representability still applies | Variadic/null semantics preserved; backend value/type representability still applies |
| `fn.contains` | mapped* | mapped* | String/code-point mapping; native argument/result types retained | String/code-point mapping; native argument/result types retained |
| `fn.cos` | mapped* | mapped* | Numeric mapping with native result width and domain guards | Numeric mapping with native result width and domain guards |
| `fn.cosh` | mapped* | mapped* | Float64 with native special-value/domain/overflow handling | Float64 with native special-value/domain/overflow handling |
| `fn.cot` | mapped* | mapped* | Float64 with native special-value/domain/overflow handling | Float64 with native special-value/domain/overflow handling |
| `fn.current_date` | lowered* | lowered* | Stable calls are bound during query preparation | Stable calls are bound during query preparation |
| `fn.current_time` | lowered* | lowered* | Stable calls are bound during query preparation | Stable calls are bound during query preparation |
| `fn.date_bin` | mapped* | mapped* | Three arguments: positive fixed microsecond interval and timezone-free microsecond source/origin | Three arguments: positive fixed microsecond interval and timezone-free microsecond source/origin |
| `fn.date_part` | mapped* | mapped* | Supported literal units on timezone-free timestamps of at most microsecond precision | Supported literal units on timezone-free timestamps of at most microsecond precision |
| `fn.date_trunc` | mapped* | mapped* | Supported literal units on timezone-free timestamps of at most microsecond precision | Supported literal units on timezone-free timestamps of at most microsecond precision |
| `fn.decode` | mapped* | mapped* | Strict hex/base64/base64pad decoding with native padding and trailing-bit validation | Strict hex/base64/base64pad decoding with native padding and trailing-bit validation |
| `fn.degrees` | mapped* | mapped* | Numeric mapping with native result width and domain guards | Numeric mapping with native result width and domain guards |
| `fn.digest` | mapped* | mapped* | Literal md5/sha224/sha256/sha384/sha512; type-directed algorithm specialization | Literal md5/sha224/sha256/sha384/sha512; type-directed algorithm specialization |
| `fn.empty` | mapped* | mapped* | Flat boolean, signed-integer, or text lists; nulls, duplicates, and native order preserved | Flat boolean, signed-integer, or text lists; nulls, duplicates, and native order preserved |
| `fn.encode` | mapped* | mapped* | Literal hex/base64/base64pad format; binary or UTF-8 text input | Literal hex/base64/base64pad format; binary or UTF-8 text input |
| `fn.ends_with` | mapped* | mapped* | String/code-point mapping; native argument/result types retained | String/code-point mapping; native argument/result types retained |
| `fn.exp` | mapped* | mapped* | Float64 with native special-value/domain/overflow handling | Float64 with native special-value/domain/overflow handling |
| `fn.factorial` | mapped* | mapped* | Numeric mapping with native result width and domain guards | Numeric mapping with native result width and domain guards |
| `fn.find_in_set` | mapped* | mapped* | String/code-point mapping; native argument/result types retained | String/code-point mapping; native argument/result types retained |
| `fn.flatten` | mapped* | mapped* | Flat boolean, signed-integer, or text lists; nulls, duplicates, and native order preserved | Flat boolean, signed-integer, or text lists; nulls, duplicates, and native order preserved |
| `fn.floor` | mapped* | mapped* | Numeric mapping with native result width and domain guards | Numeric mapping with native result width and domain guards |
| `fn.from_unixtime` | mapped* | mapped* | Single Int64 epoch argument; text parsing and timezone overloads stay native; database timestamp range applies | Single Int64 epoch argument; text parsing and timezone overloads stay native; database timestamp range applies |
| `fn.gcd` | mapped* | mapped* | Numeric mapping with native result width and domain guards | Numeric mapping with native result width and domain guards |
| `fn.generate_series` | mapped* | mapped* | Int64 bounds and optional nonzero literal step; endpoint inclusion preserved | Int64 bounds and optional nonzero literal step; endpoint inclusion preserved |
| `fn.get_field` | mapped* | mapped* | Type-directed field names and nested values; native DuckDB records and schema-directed PostgreSQL JSONB | Type-directed field names and nested values; native DuckDB records and schema-directed PostgreSQL JSONB |
| `fn.greatest` | mapped* | mapped* | Variadic/null semantics preserved; backend value/type representability still applies | Variadic/null semantics preserved; backend value/type representability still applies |
| `fn.initcap` | mapped* | mapped* | Rust Unicode alphanumeric word boundaries and per-character case conversion | Rust Unicode alphanumeric word boundaries and per-character case conversion |
| `fn.isnan` | mapped* | mapped* | Numeric mapping with native result width and domain guards | Numeric mapping with native result width and domain guards |
| `fn.iszero` | mapped* | mapped* | Numeric mapping with native result width and domain guards | Numeric mapping with native result width and domain guards |
| `fn.lcm` | mapped* | mapped* | Numeric mapping with native result width and domain guards | Numeric mapping with native result width and domain guards |
| `fn.least` | mapped* | mapped* | Variadic/null semantics preserved; backend value/type representability still applies | Variadic/null semantics preserved; backend value/type representability still applies |
| `fn.left` | mapped* | mapped* | Literal Int32-range position/count; substring length must be nonnegative | Literal Int32-range position/count; substring length must be nonnegative |
| `fn.levenshtein` | mapped* | mapped* | Unicode code-point edit distance using a recursive SQL dynamic-programming row | Unicode code-point edit distance using a recursive SQL dynamic-programming row |
| `fn.ln` | mapped* | mapped* | Numeric mapping with native result width and domain guards | Numeric mapping with native result width and domain guards |
| `fn.log` | mapped* | mapped* | Numeric mapping with native result width and domain guards | Numeric mapping with native result width and domain guards |
| `fn.log10` | mapped* | mapped* | Numeric mapping with native result width and domain guards | Numeric mapping with native result width and domain guards |
| `fn.log2` | mapped* | mapped* | Numeric mapping with native result width and domain guards | Numeric mapping with native result width and domain guards |
| `fn.lower` | mapped* | mapped* | Rust Unicode full case conversion, including expansions and contextual final sigma | Rust Unicode full case conversion, including expansions and contextual final sigma |
| `fn.lpad` | mapped* | mapped* | Unicode 17 grapheme truncation and code-point padding; literal Int32-range length | Unicode 17 grapheme truncation and code-point padding; literal Int32-range length |
| `fn.ltrim` | mapped* | mapped* | String/code-point mapping; native argument/result types retained | String/code-point mapping; native argument/result types retained |
| `fn.make_array` | mapped* | mapped* | One or more scalar boolean, signed-integer, or text arguments | One or more scalar boolean, signed-integer, or text arguments |
| `fn.make_date` | mapped* | mapped* | Integer year/month/day construction within database date range | Integer year/month/day construction within database date range |
| `fn.make_time` | mapped* | mapped* | Integer hour and literal minute/second in 0..59 | Integer hour and literal minute/second in 0..59 |
| `fn.map` | mapped* | mapped* | Typed maps; DuckDB native MAP and PostgreSQL ordered JSONB entries with exact scalar leaves | Typed maps; DuckDB native MAP and PostgreSQL ordered JSONB entries with exact scalar leaves |
| `fn.map_entries` | mapped* | mapped* | Typed maps; DuckDB native MAP and PostgreSQL ordered JSONB entries with exact scalar leaves | Typed maps; DuckDB native MAP and PostgreSQL ordered JSONB entries with exact scalar leaves |
| `fn.map_extract` | mapped* | mapped* | Typed maps; DuckDB native MAP and PostgreSQL ordered JSONB entries with exact scalar leaves | Typed maps; DuckDB native MAP and PostgreSQL ordered JSONB entries with exact scalar leaves |
| `fn.map_keys` | mapped* | mapped* | Typed maps; DuckDB native MAP and PostgreSQL ordered JSONB entries with exact scalar leaves | Typed maps; DuckDB native MAP and PostgreSQL ordered JSONB entries with exact scalar leaves |
| `fn.map_values` | mapped* | mapped* | Typed maps; DuckDB native MAP and PostgreSQL ordered JSONB entries with exact scalar leaves | Typed maps; DuckDB native MAP and PostgreSQL ordered JSONB entries with exact scalar leaves |
| `fn.md5` | mapped* | mapped* | Core hash implementation; native binary/text arguments | Core hash implementation; native binary/text arguments |
| `fn.named_struct` | mapped* | mapped* | Type-directed field names and nested values; native DuckDB records and schema-directed PostgreSQL JSONB | Type-directed field names and nested values; native DuckDB records and schema-directed PostgreSQL JSONB |
| `fn.nanvl` | mapped* | mapped* | Numeric mapping with native result width and domain guards | Numeric mapping with native result width and domain guards |
| `fn.now` | lowered* | lowered* | Stable calls are bound during query preparation | Stable calls are bound during query preparation |
| `fn.nullif` | mapped* | mapped* | Variadic/null semantics preserved; backend value/type representability still applies | Variadic/null semantics preserved; backend value/type representability still applies |
| `fn.nvl` | mapped* | mapped* | Variadic/null semantics preserved; backend value/type representability still applies | Variadic/null semantics preserved; backend value/type representability still applies |
| `fn.nvl2` | mapped* | mapped* | Variadic/null semantics preserved; backend value/type representability still applies | Variadic/null semantics preserved; backend value/type representability still applies |
| `fn.octet_length` | mapped* | mapped* | String/code-point mapping; native argument/result types retained | String/code-point mapping; native argument/result types retained |
| `fn.overlay` | mapped* | mapped* | Character-based overlay; portable native wrapper fixes upstream UTF-8 slicing panic and boundary handling | Character-based overlay; portable native wrapper fixes upstream UTF-8 slicing panic and boundary handling |
| `fn.pi` | mapped* | mapped* | Numeric mapping with native result width and domain guards | Numeric mapping with native result width and domain guards |
| `fn.power` | mapped* | mapped* | Float64 IEEE power; integer/decimal overloads stay native | Float64 literal exponents 0, 0.5, 1, 2 with exact IEEE boundaries; other powers stay native |
| `fn.radians` | mapped* | mapped* | Numeric mapping with native result width and domain guards | Numeric mapping with native result width and domain guards |
| `fn.random` | mapped* | mapped* | Volatile backend implementation; each argument evaluated once | Volatile backend implementation; each argument evaluated once |
| `fn.range` | mapped* | mapped* | Int64 bounds and optional nonzero literal step; endpoint inclusion preserved | Int64 bounds and optional nonzero literal step; endpoint inclusion preserved |
| `fn.regexp_count` | mapped* | mapped* | Literal common Rust/RE2/POSIX subset; no captures, alternation, Unicode classes, lazy quantifiers, empty matches, or replacement backreferences | Literal common Rust/RE2/POSIX subset; no captures, alternation, Unicode classes, lazy quantifiers, empty matches, or replacement backreferences |
| `fn.regexp_instr` | mapped* | mapped* | Literal common Rust/RE2/POSIX subset; no captures, alternation, Unicode classes, lazy quantifiers, empty matches, or replacement backreferences | Literal common Rust/RE2/POSIX subset; no captures, alternation, Unicode classes, lazy quantifiers, empty matches, or replacement backreferences |
| `fn.regexp_like` | mapped* | mapped* | Literal common Rust/RE2/POSIX subset; no captures, alternation, Unicode classes, lazy quantifiers, empty matches, or replacement backreferences | Literal common Rust/RE2/POSIX subset; no captures, alternation, Unicode classes, lazy quantifiers, empty matches, or replacement backreferences |
| `fn.regexp_match` | mapped* | mapped* | Literal common Rust/RE2/POSIX subset; no captures, alternation, Unicode classes, lazy quantifiers, empty matches, or replacement backreferences | Literal common Rust/RE2/POSIX subset; no captures, alternation, Unicode classes, lazy quantifiers, empty matches, or replacement backreferences |
| `fn.regexp_replace` | mapped* | mapped* | Literal common Rust/RE2/POSIX subset; no captures, alternation, Unicode classes, lazy quantifiers, empty matches, or replacement backreferences | Literal common Rust/RE2/POSIX subset; no captures, alternation, Unicode classes, lazy quantifiers, empty matches, or replacement backreferences |
| `fn.repeat` | mapped* | mapped* | Literal Int32-range position/count; substring length must be nonnegative | Literal Int32-range position/count; substring length must be nonnegative |
| `fn.replace` | mapped* | mapped* | String/code-point mapping; native argument/result types retained | String/code-point mapping; native argument/result types retained |
| `fn.reverse` | mapped* | mapped* | String/code-point mapping; native argument/result types retained | String/code-point mapping; native argument/result types retained |
| `fn.right` | mapped* | mapped* | Literal Int32-range position/count; substring length must be nonnegative | Literal Int32-range position/count; substring length must be nonnegative |
| `fn.round` | mapped* | mapped* | Float64 and optional literal precision in -15..15 | Float64 single-argument overload; binary rescaling precision overload stays native |
| `fn.rpad` | mapped* | mapped* | Unicode 17 grapheme truncation and code-point padding; literal Int32-range length | Unicode 17 grapheme truncation and code-point padding; literal Int32-range length |
| `fn.rtrim` | mapped* | mapped* | String/code-point mapping; native argument/result types retained | String/code-point mapping; native argument/result types retained |
| `fn.sha224` | mapped* | mapped* | Core-SQL SHA-2 compression; binary/text input, multiple message blocks, no extension | Core hash implementation; native binary/text arguments |
| `fn.sha256` | mapped* | mapped* | Core hash implementation; native binary/text arguments | Core hash implementation; native binary/text arguments |
| `fn.sha384` | mapped* | mapped* | Core-SQL SHA-2 compression; binary/text input, multiple message blocks, no extension | Core hash implementation; native binary/text arguments |
| `fn.sha512` | mapped* | mapped* | Core-SQL SHA-2 compression; binary/text input, multiple message blocks, no extension | Core hash implementation; native binary/text arguments |
| `fn.signum` | mapped* | mapped* | Numeric mapping with native result width and domain guards | Numeric mapping with native result width and domain guards |
| `fn.sin` | mapped* | mapped* | Numeric mapping with native result width and domain guards | Numeric mapping with native result width and domain guards |
| `fn.sinh` | mapped* | mapped* | Float64 with native special-value/domain/overflow handling | Float64 with native special-value/domain/overflow handling |
| `fn.split_part` | mapped* | mapped* | Nonzero literal Int32-range field index | Nonzero literal Int32-range field index |
| `fn.sqrt` | mapped* | mapped* | Numeric mapping with native result width and domain guards | Numeric mapping with native result width and domain guards |
| `fn.starts_with` | mapped* | mapped* | String/code-point mapping; native argument/result types retained | String/code-point mapping; native argument/result types retained |
| `fn.string_to_array` | mapped* | mapped* | Two/three arguments; empty text, null delimiter, and null replacement preserved | Two/three arguments; empty text, null delimiter, and null replacement preserved |
| `fn.strpos` | mapped* | mapped* | String/code-point mapping; native argument/result types retained | String/code-point mapping; native argument/result types retained |
| `fn.struct` | mapped* | mapped* | Type-directed field names and nested values; native DuckDB records and schema-directed PostgreSQL JSONB | Type-directed field names and nested values; native DuckDB records and schema-directed PostgreSQL JSONB |
| `fn.substr` | mapped* | mapped* | Literal Int32-range position/count; substring length must be nonnegative | Literal Int32-range position/count; substring length must be nonnegative |
| `fn.substr_index` | mapped* | mapped* | Native left/right occurrence counting, including overlapping delimiters | Native left/right occurrence counting, including overlapping delimiters |
| `fn.tan` | mapped* | mapped* | Numeric mapping with native result width and domain guards | Numeric mapping with native result width and domain guards |
| `fn.tanh` | mapped* | mapped* | Numeric mapping with native result width and domain guards | Numeric mapping with native result width and domain guards |
| `fn.to_char` | mapped* | mapped* | ISO date/time literal formats on timezone-free date/timestamp input | ISO date/time literal formats on timezone-free date/timestamp input |
| `fn.to_date` | mapped* | mapped* | Single timezone-free date/timestamp argument; parsing/timezone conversion stays native | Single timezone-free date/timestamp argument; parsing/timezone conversion stays native |
| `fn.to_hex` | mapped* | mapped* | String/code-point mapping; native argument/result types retained | String/code-point mapping; native argument/result types retained |
| `fn.to_local_time` | mapped* | mapped* | Single timezone-free date/timestamp argument; parsing/timezone conversion stays native | Single timezone-free date/timestamp argument; parsing/timezone conversion stays native |
| `fn.to_time` | mapped* | mapped* | Time input of at most microsecond precision; text parsing stays native | Time input of at most microsecond precision; text parsing stays native |
| `fn.to_timestamp` | mapped* | mapped* | Single Int64 epoch argument; text parsing and timezone overloads stay native; database timestamp range applies | Single Int64 epoch argument; text parsing and timezone overloads stay native; database timestamp range applies |
| `fn.to_timestamp_micros` | mapped* | mapped* | Single Int64 epoch argument; text parsing and timezone overloads stay native; database timestamp range applies | Single Int64 epoch argument; text parsing and timezone overloads stay native; database timestamp range applies |
| `fn.to_timestamp_millis` | mapped* | mapped* | Single Int64 epoch argument; text parsing and timezone overloads stay native; database timestamp range applies | Single Int64 epoch argument; text parsing and timezone overloads stay native; database timestamp range applies |
| `fn.to_timestamp_nanos` | mapped* | mapped* | Single Int64 epoch argument; text parsing and timezone overloads stay native; database timestamp range applies | Microsecond-aligned literal epoch; arbitrary nanoseconds cannot be preserved by PostgreSQL timestamps |
| `fn.to_timestamp_seconds` | mapped* | mapped* | Single Int64 epoch argument; text parsing and timezone overloads stay native; database timestamp range applies | Single Int64 epoch argument; text parsing and timezone overloads stay native; database timestamp range applies |
| `fn.to_unixtime` | mapped* | mapped* | Timezone-free timestamp of at most microsecond precision | Timezone-free timestamp of at most microsecond precision |
| `fn.translate` | mapped* | mapped* | Unicode 17 extended grapheme translation; first duplicate match and deletion preserved | Unicode 17 extended grapheme translation; first duplicate match and deletion preserved |
| `fn.trunc` | mapped* | mapped* | Float64 and optional literal precision in -15..15 | Float64 single-argument overload; binary rescaling precision overload stays native |
| `fn.union_extract` | mapped* | mapped* | Typed union variants, including null payloads and noncontiguous Arrow type IDs | Typed union variants, including null payloads and noncontiguous Arrow type IDs |
| `fn.union_tag` | mapped* | mapped* | Typed union variants, including null payloads and noncontiguous Arrow type IDs | Typed union variants, including null payloads and noncontiguous Arrow type IDs |
| `fn.upper` | mapped* | mapped* | Rust Unicode full case conversion, including expansions and contextual final sigma | Rust Unicode full case conversion, including expansions and contextual final sigma |
| `fn.uuid` | mapped* | mapped* | Volatile backend implementation; each argument evaluated once | Volatile backend implementation; each argument evaluated once |
| `fn.version` | lowered* | lowered* | Preparation folds the native runtime version; database version() would describe a different engine | Preparation folds the native runtime version; database version() would describe a different engine |

*Lowered functions resolve Arrow schema/query metadata or become casts during compilation. Runtime mappings are specific to arity, argument types, and options. Calls without a matching mapping execute natively; SQL-only compilation rejects them. Every listed function has a native implementation. Aliases resolve to the same identity.
