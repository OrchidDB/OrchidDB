# Maintaining portable scalar functions

Contributor reference for catalog generation, SQL mappings, and regression tests.
For query usage, see [Portable scalar functions](portable-functions.md).

## Catalog generation and capability APIs

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
