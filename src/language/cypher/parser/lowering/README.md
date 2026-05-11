# Cypher Parser Lowering

This directory is the implementation boundary for lowering the generated
ANTLR Cypher parse tree into `crate::language::cypher::ast`.

The current files are intentionally split by grammar family so each file can
be compared against Ladybug, Lance Graph, Grafeo, and reference Cypher
behavior in one focused pass.

Expected ownership:

- `visitor.rs`: ANTLR visitor entry point and traversal orchestration.
- `context.rs`, `frames.rs`, `diagnostics.rs`: shared lowering state,
  typed intermediate values, and error construction.
- `statements.rs`, `queries.rs`, `clauses.rs`, `updating.rs`: statement,
  query-part, reading-clause, and updating-clause boundaries.
- `patterns.rs`, `labels.rs`, `ranges.rs`, `properties.rs`: graph pattern
  syntax.
- `projections.rs`: `RETURN`, `WITH`, `ORDER BY`, `SKIP`, `LIMIT`.
- `expressions.rs`, `operators.rs`, `arithmetic.rs`, `predicates.rs`,
  `functions.rs`, `cases.rs`, `collections.rs`, `literals.rs`,
  `parameters.rs`: value expression syntax.
- `procedures.rs`, `subqueries.rs`: `CALL`, `YIELD`, and subquery syntax.
- `names.rs`, `schema.rs`, `text.rs`, `source.rs`: identifier, namespace,
  parse-text, and source-location helpers.
