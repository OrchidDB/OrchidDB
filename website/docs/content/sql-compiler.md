# Advanced mapping protocol

`orchid_query(request_json)` accepts the existing version-1 mapping protocol for
Cypher, Gremlin, and SPARQL. Requests contain `version`, `language`, `query`, and
source/mapping declarations. DuckDB binds declared tables or views and supplies
their actual schemas; caller-supplied column declarations are not authoritative.

The extension requires the host DuckDB as the execution engine.
`orchid_compile(request_json)` exposes compilation metadata. Compiler errors include
an `error` and any frontend `classification`; schema binding errors raise DuckDB
exceptions. This protocol also carries advanced mappings and typed bindings.

See the [SPARQL example](sparql.md) and the repository's
[compiler source](https://github.com/OrchidDB/OrchidDB/blob/main/src/compiler.rs)
for the request schema. Prefer native graph DDL and statements for ordinary property
graph queries. The retained library compiler is an internal reusable component,
not a separately packaged client product.
