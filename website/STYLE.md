# Crabgraph editorial style

This guide adapts the technical clarity and source discipline of the Chicago Manual of Style for Crabgraph. When this guide is silent, prefer Chicago style, then the conventions of the language or API being discussed.

## Purpose

Write for database engineers, graph practitioners, and infrastructure teams evaluating an early project. Help readers understand the architecture, its current proof points, and its unfinished edges without marketing fog.

## Voice

- Precise, calm, and technical.
- Direct about what works and what remains in development.
- Concrete before abstract. Show the query, plan, engine, or measurement.
- Confident about the design, conservative about implementation status.
- Interested in interoperability, not in declaring a winner among data systems.

## Mechanics

- Use sentence case for headings.
- Use active voice and short declarative sentences.
- Use the serial comma.
- Do not use em dashes. Use a period, comma, colon, or parentheses.
- Define a specialist term on first use when the surrounding sentence does not make it clear.
- Use `SQL`, `IR`, and `API` without expansion. Expand less familiar abbreviations on first use.
- Write `Cypher`, `Gremlin`, `SPARQL`, `Graph IR`, `DataFusion`, `DuckDB`, and `Postgres` exactly as shown.
- Write `SQL island` as two words. An island is a region of a graph plan that can execute as relational SQL without changing its semantics.
- Put code, identifiers, query steps, and configuration values in code style.
- Give measurements as numerator, denominator, and percentage. Name the corpus, execution path, and date or benchmark snapshot.
- Prefer `available`, `partial`, and `planned` for status labels. Do not use vague maturity labels such as “enterprise-ready.”

## Product language

- Describe Crabgraph as a data fusion layer or graph query planner, not as a graph database.
- Say that it transpiles graph-language plans into SQL islands. Do not imply that every query becomes one SQL statement.
- Say that the data layer is unopinionated. Users map graph concepts onto relational tables they already own.
- Lead with the bring-your-own-schema contract. Existing Iceberg tables, warehouse data, relational tables, SQL queries, and views can provide the graph shape without a graph-data copy.
- Say that DuckDB is the best-developed target and the default feature. Other SQL targets are possible through the executor boundary, but compatibility work remains.
- Describe SPARQL as a vocabulary frontend over the same property-graph and relational mappings used by other languages. An ontology mapping resolves classes and predicates to node labels, relationship types, and properties before SQL lowering. Do not imply that users must reshape their data into RDF triples or quads.
- Distinguish interpreter accuracy, SQL-to-interpreter agreement, SQL pushdown, and expected-output accuracy. They answer different questions.
- Never present imported-corpus accuracy as standards certification.

## Claims and evidence

- Tie implementation claims to code in `src/` or tests in `tests/`.
- Tie coverage figures to the checked-in benchmark notes under `docs/`.
- State the denominator beside every percentage.
- Mark Postgres compatibility, mutations in SQL, complete Cypher coverage, and complete Gremlin coverage as unfinished.
- Prefer “designed to” for architectural direction that is not yet a release guarantee.

## Words to avoid

Avoid: seamless, revolutionary, robust, unlock, supercharge, effortless, cutting-edge, game-changing, and production-ready.
