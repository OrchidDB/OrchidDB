# OrchidDB for JavaScript / TypeScript

A Node client with caller-owned Arrow execution. Build this checkout locally:

```sh
make native
(cd clients/js && npm ci && npm run build)
node clients/js/examples/duckdb-wasm.mjs
```

```ts
import { Connection, batches } from '@orchiddb/client';

// schema contains tables, nodes, edges, and other mapping metadata only.
// engine is your Arrow adapter on an existing database connection.
const graph = new Connection(engine, schema);
try {
  const result = await graph.query(
    'MATCH (p:Person) WHERE p.name=$name RETURN p.name AS name',
    {parameters: {name: 'Ada'}});
  for await (const batch of batches(result)) console.log(batch.toArray());
} finally {
  graph.close();
}
```

Use a local package dependency (`file:/path/to/OrchidDB/clients/js`) for this
source API. Older published packages have the previous interface.
[The runnable example](examples/duckdb-wasm.mjs) supplies a real DuckDB-Wasm
connection and Arrow adapter. The library itself bundles no database driver.
Node 20+ is required; the native runtime is not a browser/Wasm compiler.

The connection retains schema separately. Query options are `parameters`,
`language` (`cypher`, `gremlin`, `sparql`), and `authorization`. Schema files
containing those fields or query text are rejected. No public Compiler or
compile-to-SQL API is exported. Use bigint for signed int64 parameters that
cannot be represented exactly as JavaScript Numbers.

`batches(result)` closes its result on completion, failure, or early exit.
Closing a graph/result leaves the database connection and transaction intact.
Backend adapters implement `ExecutionEngine`; they receive internal `SqlWork`
and return an Arrow schema, async batch iterator, and idempotent `close`.

A third constructor argument can supply named engines for federation; schema
holds engine routing. Federated final batches are collected before returning.
`RemoteEngine(adapter, options)` supports Quickwit and Elasticsearch. Close
remote sessions separately. `generateStatistics`, `saveStatistics`,
`loadStatistics`, and `clearStatistics` operate on the registered schema.

[Source builds](../README.md)
