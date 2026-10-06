# Statistics and planning

DuckDB owns optimization of relational plans and source scan estimates. Use
DuckDB's normal statistics and `EXPLAIN` facilities for mapped tables. Statistics
for external sources depend on the source extension.

The shared compiler retains its existing optional statistics model for advanced
mappings. Native graph DDL does not automatically create or refresh that snapshot.
Statistics affect planning choices, not key validity or graph semantics.
