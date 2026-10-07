# OrchidDB for JavaScript and TypeScript

Source builds use the shared core in this monorepo. See [build and test instructions](../README.md).

Compile Cypher, Gremlin text, or SPARQL to SQL. Execute on your own database and consume Apache Arrow record batches. No database is bundled; query results remain with your application; explicit statistics generation sends bounded samples to shared Rust code.

```sh
npm install @orchiddb/client@0.3.0 apache-arrow@17
```

For SQL compilation alone, install only `@orchiddb/client`. Apache Arrow is an
optional peer dependency: install it when using the Arrow interfaces (including
the TypeScript declarations). Database drivers remain application dependencies.

Version 0.3.0 is published on npm and bundles compilers for Linux ARM64/x86_64 and macOS ARM64/x86_64. No native library path or Rust build is required on those platforms. Node 20+; this package is not a browser/Wasm compiler.

Run the example against published packages:

```sh
cd examples
npm install
npm start
```

The example supplies its own DuckDB-Wasm and Arrow dependencies and checks graph values, nulls and connection reuse.

```ts
import { Compiler, batches } from '@orchiddb/client';
const compiler = new Compiler();
const plan = compiler.compile({
  version: 1, dialect: 'duckdb', language: 'cypher',
  query: 'MATCH (p:Person) RETURN p.name',
  tables: [{ name: 'people', columns: [
    { name: 'id', data_type: 'int64' }, { name: 'name', data_type: 'string' }
  ] }],
  nodes: [{ label: 'Person', table: 'people', id: 'id', properties: {name: 'name'} }]
});
// Your engine owns the connection, schema, extensions, UDFs, transactions and caches.
const result = await yourArrowEngine.execute(plan);
for await (const batch of batches(result)) console.log(batch.numRows);
```

Implement `ExecutionEngine.execute()` to return `ArrowResult`: a schema, async batch iterator, and an idempotent `close()` releasing only result resources. Its connection stays caller-owned. Batches remain valid according to the producer's documented lifetime; copy/retain them yourself before advancing if required. `batches()` closes on completion, errors, or early exit.

See [the runnable DuckDB-Wasm example](examples/duckdb-wasm.mjs), also exercised by `npm test`. DuckDB-Wasm produces Arrow directly (IPC decoding still has costs). DuckDB's Node Neo driver is **not** silently adapted via row objects. Supply a genuine Arrow producer. Compilation is synchronous; use workers if blocking the event loop matters. Pass signed 64-bit integer parameters as JavaScript `bigint` (for example `9007199254740993n`). Unsafe integer Numbers, non-finite Numbers, and bigint values outside signed int64 are rejected before compilation, including in nested parameter values. Ordinary finite fractional Numbers remain supported. SQL currently specializes parameter values; cached plans must include parameters, schema, mappings, functions and dialect. PostgreSQL and mixed-engine SQL islands are supported through caller-owned engines. ClickHouse is not implemented.

## Release

Build and stage the shared native library locally with `make native` from the repository root. Package and publish only validated artifacts from a clean monorepo revision; GitHub Actions builds are not used.

SPARQL compile requests accept `rdf: RdfMapping[]` and an optional `dataset`.
Rules map subjects, predicates, objects, and graphs to the registered application
tables; `RdfTermMapping` provides typed column, template, blank, literal, and
constant constructors. No RDF storage table is required. Use a native compiler
built with the shared RDF mapping API for these requests.

## Generate statistics once

```js
import { Compiler, asyncDuckDBEngine } from '@orchiddb/client';
const compiler = new Compiler();
const engine = asyncDuckDBEngine(connection); // worker-backed AsyncDuckDB connection
const analysis = await compiler.generateStatistics(request, engine);
console.log(analysis.report);
const plan = compiler.compile(request);       // automatically uses retained catalog
console.log(plan.representation_selections);  // full diagnostics are preserved
compiler.saveStatistics('statistics.json');
compiler.clearStatistics();
compiler.loadStatistics('statistics.json');
compiler.close();                            // release catalog, not database session
```

One explicit operation can issue multiple bounded database reads. Shared Rust
code plans collection, computes statistics, and selects cheaper equivalent
sources. The client transfers Arrow IPC batches and retains a native catalog
handle, avoiding snapshot serialization on every compile. The same catalog is
used for Cypher, Gremlin, and SPARQL. No tiers, background refreshes, or optimizer
modes are exposed. Regenerate explicitly after data changes; mapping identity
is validated by the shared implementation.

Custom `ExecutionEngine` adapters provide `executeStatistics(task, signal)`.
They must enforce cancellation and deadlines while obtaining and reading the
result. `asyncDuckDBEngine` adapts a worker-backed DuckDB-WASM connection and
cancels its active stream on abort. The blocking WASM example cannot enforce
wall-clock cancellation, so its adapter intentionally reports unsupported
statistics execution instead of running an unbounded fallback. Normal queries
remain supported. Coverage reports include collection errors and skipped work.
The driver caps submitted rows/bytes and closes leases on every path.

`statisticsCommand(command)` exposes the same coordinator protocol for
application-owned sessions. `saveStatistics`/`loadStatistics` persist portable
snapshots. Retain one compiler per independently analyzed mapping; `close`
releases its catalog. `apache-arrow` is required when generating statistics.

## Permission pushdown

The compiler accepts a provider-neutral flat effective-grants relation. Node scopes match resource IDs against node source columns, and multiple scopes combine with OR membership filters:

```ts
import { Compiler, authorization, permissionRelation, permissionScope } from '@orchiddb/client';
const compiler = new Compiler();
const direct = permissionRelation('effective_grants', 'document', 'view');
const project = permissionRelation('effective_grants', 'project', 'view');
const plan = compiler.compile({
  ...request,
  authorization: authorization('user', 'alice'),
  tables: [...request.tables, { name: 'effective_grants', columns: grantColumns }],
  nodes: [{ ...request.nodes[0], permission_scopes: [
    permissionScope('id', direct), permissionScope('project_id', project)
  ] }]
});
```

Include each permission source in `tables`, and map each scope column as a node property. The relation must contain effective grants for the supplied principal; this client does not connect to or synchronize a permission service.

### Quickwit and Elasticsearch

Use `RemoteEngine` beside your existing SQL adapter. Your request's engine
registry and graph mapping identify where each table or search index lives.

```js
import {Compiler, RemoteEngine, queryFederated} from '@orchiddb/client';

const compiler = new Compiler();
const search = new RemoteEngine(compiler, 'quickwit', {
  endpoint: 'http://localhost:7280', page_size: 1000,
});
try {
  await queryFederated(compiler, request, new Map([
    ['db', sqlEngine], ['search', search],
  ]), async result => {
    for await (const batch of result) consume(batch);
  });
} finally {
  search.close();
  compiler.close();
}
```

Use `'elasticsearch'` and its endpoint for Elasticsearch. See the
[search engine guide](https://docs.orchiddb.com/remote-engines.html) for
complete graph mappings and queries. Install the optional `apache-arrow` peer.
Credentials belong in session options, for example
`authentication: {type: 'bearer', token}`; basic authentication and API keys are
also supported. Remote calls use the native transport and block the calling
thread; use a worker thread in latency-sensitive servers. Federation closes
result leases; SQL connections and remote sessions remain caller-owned.
Errors propagate without returning partial results.

A custom request engine can implement `id`, `dialect`, and
`executeRequests(requests, columns): Promise<ArrowResult>`. Dependent operations
and SQL continuations are composed automatically. Arrow exchange retains large
integers and JSON validity. To run the live service tests locally, set
`ORCHIDDB_REMOTE_FIXTURE` to the shared release fixture JSON and run `npm test`.
