# Client APIs

OrchidDB's client API design centers on three operations: open a graph, define
its mapping, and execute a query. Applications receive tabular results while
the embedded engine handles graph planning and execution.

| Language | Integration model |
| --- | --- |
| Rust | Native library embedded directly in the application process. |
| Python | Python bindings for notebooks, analysis, and data pipelines. |
| JavaScript / TypeScript | A typed Node.js API for services and data applications. |
| Java | JVM integration for applications using the Java ecosystem. |

This table describes the client API design. The installation guide documents
the available Rust library and command-line interface. The repository also
contains the JVM execution bridge and TinkerPop provider.

## Shared API shape

Each language binding follows the same conceptual flow:

1. Open the engine within your application.
2. Map source tables to nodes, relationships, and properties.
3. Submit a Cypher, Gremlin, or SPARQL query through the appropriate frontend.
4. Consume the result in the application's tabular data tools.

## Rust

See the [Rust API](rust-api.md) for executable examples and the
[installation guide](installation.md) for source dependencies.

## Data lake access

Use [mapped graphs](mapped-graphs.md) to describe relationships over tables
and views. Iceberg access uses DuckDB's Iceberg extension and your catalog and
credential configuration. OrchidDB's graph mapping sits above that source
connection; it does not supply a separate Iceberg catalog or distributed
federation service.
