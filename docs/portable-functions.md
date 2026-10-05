# Portable scalar functions

Use `fn.<name>` for a function whose typed native implementation remains
available when its SQL backend cannot execute it. This extends the portable
`vector.*` and `json.*` function mechanism to the complete enabled DataFusion
scalar and nested-function catalogs. Native aliases work too; for example,
`fn.char_length` resolves to `fn.character_length`.

```cypher
MATCH (d:Document)
RETURN fn.sqrt(d.score), fn.reverse(d.title)
```

The same names work in SQL expressions in query-backed mappings and computed
relationships. Existing language-defined functions and engine-catalog calls
retain their names and semantics. These are scalar functions; aggregate,
window and table functions continue to use their existing catalogs.

## Execution and SQL support

All 165 enabled catalog functions have a DuckDB and PostgreSQL SQL path.
Arrow type/metadata inspection and runtime version resolve at compilation;
Arrow casts lower to typed SQL casts. Stable query-time calls can be bound by
query preparation. Other functions use direct built-ins or SQL rewrites.

The catalog retains native argument coercion, result types, volatility and
short-circuit behavior. Two portable native corrections are intentional:
`fn.overlay` uses Unicode character positions, a positive start and nonnegative
length, fixing the pinned upstream implementation's UTF-8 slicing panic and
out-of-range behavior; `fn.map` broadcasts constant key/value lists alongside
column arguments, fixing the upstream mixed scalar/array path.

Mappings cover arithmetic, null handling, Unicode strings, hashing/encoding,
arrays, maps, records, unions, integer series, and selected temporal and regex
overloads. Array rewrites preserve null elements, duplicates and ordering.
Numeric guards preserve NaN/infinity behavior. Repeated SQL expressions bind
arguments once, and text-to-binary coercion copies UTF-8 bytes.

Availability is checked per call, using its arity, Arrow types, and literal
options. For example, `fn.regexp_like(title, '[a-z]+')` can run in either engine,
while `fn.regexp_like(title, '\w+')` remains native because Unicode character
classes differ. The portable identity survives optimization so rewriting a UDF
into a native regex operator cannot bypass this check.

The [complete reviewed catalog](portable-function-catalog.md) records the
supported signatures for every function. Unicode case conversion uses the
native Rust case tables, including multi-character expansions and contextual
Greek sigma. Translation and padding use Unicode 17 grapheme rules from the
pinned native segmentation dependency. Edit distance counts Unicode code points.
DuckDB's missing SHA-2 algorithms use SQL compression rounds. These rewrites
need no database extensions; complex rewrites can cost more than a built-in.

Structured calls specialize their SQL by Arrow field names and types. DuckDB
uses native MAP/STRUCT/UNION values. PostgreSQL uses schema-directed JSONB:
records preserve field names, maps retain entry order, and unions retain tags
separately from nullable payloads. Textual scalar leaves preserve exact numbers,
non-finite floats and binary bytes. Results rebuild the original Arrow schema.

Inspect the current catalog and per-engine availability:

```sh
cargo run --example portable_function_catalog
```

Rust callers can use `ir::functions::portable::function("fn.sqrt")` and
`portable::capabilities()`. Capability flags mean at least one supported SQL call or compile-time lowering;
`preparation` identifies schema/query-state lowering. Static templates are
available through `logical::definition(&udf).sql_mapping(dialect, arity)`.
Use `sql_mapping_for_call(dialect, args, schema)` for type-directed mappings. `portable::validate_call(name, args, schema, dialect)` checks the
additional call restrictions, and `duckdb_note` / `postgres_note` describe the
reviewed scope. Both mixed execution and SQL-only compilation use the same
checks. Unsupported calls execute natively in mixed plans; SQL-only errors name
the function, engine, and reason.

The SQL boundary supports binary, timezone-free dates/timestamps, time values,
arrays, maps, records and tagged unions. PostgreSQL cannot represent arbitrary nanosecond timestamps or NUL text; precision checks
also apply after preparation turns functions into literals.
SQL database value ranges still apply. Regex mappings accept a case-sensitive
literal subset without captures, alternation, Unicode classes, lazy quantifiers,
or empty matches. More complex patterns retain native execution.

## Extending mappings

`LogicalFunction.sql` supplies a dialect-wide expression template.
`LogicalFunction.sql_overloads` supplies mappings keyed by `(dialect, arity)`;
these take precedence. A custom engine adapter's mapping takes precedence over
both. SQL templates are parsed and arguments substituted as AST nodes:

- `__arg0`, `__arg1`, etc. substitute individual expressions.
- A bare `__args` in a function argument list or `ARRAY[...]` expands all positional arguments.
- `__local0`, etc. are renamed to avoid capturing caller identifiers.

These placeholders do not substitute text inside SQL string literals. A single
query may use several arities of the same function. Missing arities fail SQL
placement independently rather than borrowing another overload's template.

## Verification

```sh
cargo test --features duckdb --test portable_functions
cargo test --features "duckdb postgres" --test portable_functions -- --include-ignored --nocapture
```

The ignored differential tests require DuckDB 1.5.2+ (`duckdb`) and `psql` on PATH and a local
PostgreSQL database. `GRAPH_PG_URL` selects the database; the default is
`postgres`. It creates temporary tables and compares native results with SQL
results using actual column inputs, including NULL, empty text, Unicode,
quotes, NaN, infinities and numeric domain boundaries. No permanent tables are
created. The executor test also checks actual Arrow round trips through both
backends. The ordinary tests cover complete catalog/alias lookup, an exhaustive
audit entry for each function, typed native fallback, arity coexistence,
expression substitution, lazy coalesce, and Cypher integration.

Semantic references: [DataFusion scalar functions](https://datafusion.apache.org/user-guide/sql/scalar_functions.html),
[PostgreSQL math functions](https://www.postgresql.org/docs/17/functions-math.html),
and [DuckDB text functions](https://duckdb.org/docs/stable/sql/functions/text).
The pinned native implementation and executable differential tests determine
this catalog's behavior.
