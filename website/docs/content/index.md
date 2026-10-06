# OrchidDB in DuckDB

Orchid adds Cypher, Gremlin, and existing SPARQL query and update functionality
to DuckDB. Define a graph over tables or views, then query it through your normal
DuckDB connection. Iceberg and Lance continue to provide their own scans and indexes.

Start with [installation](installation.md) and the [quickstart](quickstart.md).
The extension targets DuckDB **1.5.6**. See [conformance](conformance.md) for the
pinned upstream results and [execution](execution.md) for the architecture.
