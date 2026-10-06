# Scalar functions

The extension reuses the shared function registry and language-specific scalar
kernels. SQL-compatible expressions lower to DuckDB functions; residual expressions
use the existing kernels. Cypher, Gremlin, and SPARQL keep their own null, numeric,
string, and RDF term semantics.

Use [EXPLAIN](execution.md) to inspect relational execution. Unsupported operations
raise errors; they are not silently sent to a second database engine.
The [function reference](portable-function-catalog.md) links the implementation catalog.
