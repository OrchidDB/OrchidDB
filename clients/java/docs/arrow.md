# Arrow batch results

`graph.queryArrow(query)` returns an `ArrowResult`: `schema()`, `nextBatch()`,
`batch()` (`VectorSchemaRoot`), `dictionaries()`, and `close()`.
The schema is available before the first batch, including for an empty result.
The compiler sees query text and metadata only. Result buffers travel directly
from the application-owned engine to the Java consumer, never through the
OrchidDB Rust/JNI compiler.

## DuckDB with your connection

The application supplies the driver, allocator, and native Arrow exporter:

```java
var engine = JdbcEngine.borrowed("lake", SqlDialect.DUCKDB, connection)
    .withArrow(
        (rows, memory, batchSize) ->
            (ArrowReader) rows.unwrap(DuckDBResultSet.class)
                .arrowExportStream(memory, batchSize),
        allocator,
        2048);
var graph = new OrchidDB(compiler, PlanCache.bounded(128), engine).graph(mapping);

try (var result = graph.queryArrow(Query.cypher(
        "MATCH (p:Person) RETURN p.name AS name"))) {
    while (result.nextBatch()) {
        VectorSchemaRoot batch = result.batch();
        // Consume column vectors here; no ResultSet.getObject() per cell.
    }
}
```

See the complete [ArrowBatches example](../orchiddb-java/src/test/java/io/orchiddb/examples/ArrowBatches.java):

```sh
./scripts/build.sh
./scripts/run-example.sh ArrowBatches
```

The exporter is a typed callback, not reflection or a hard dependency on DuckDB.
Other drivers can provide their own native exporter. Non-JDBC engines can
implement `ExecutionEngine.Session.executeArrow` directly. Unsupported adapters
raise `SQLFeatureNotSupportedException`; there is no silent row-to-Arrow fallback.
Existing row-only adapters remain compatible.

## Application dependencies and JVM settings

The API brings `arrow-vector` and its dependencies. For the DuckDB native export
example, add your chosen DuckDB JDBC driver plus `org.apache.arrow:arrow-c-data`
and `org.apache.arrow:arrow-memory-netty` at version **18.3.0**, matching the API.
These driver/allocator/export dependencies are test-only in this repository;
the application chooses its memory implementation and owns the parent allocator.

Applications running on the classpath need this Arrow JVM option:

```sh
java --add-opens=java.base/java.nio=ALL-UNNAMED ...
```

Surefire and the Arrow example launcher set it for local tests/examples. Consult
[Arrow's installation guide](https://arrow.apache.org/java/current/install.html)
for modular application settings.

Arrow export does not itself enable streaming query execution. To opt into
DuckDB streaming, the caller sets `jdbc_stream_results=true` in connection
properties **before creating its connection**, as the example demonstrates.
OrchidDB never changes connection settings, autocommit, extensions or transaction
boundaries. The database may still materialize blocking operators such as sorts.
See [DuckDB result handling](https://duckdb.org/docs/current/clients/java/result_handling).

## Ownership

- Keep the connection and parent allocator alive until results close. Each query
  gets a child allocator; closing the result closes its Arrow reader, JDBC result
  and statement, then the child allocator. The parent remains caller-owned.
- `batch()` and its vectors are borrowed until the next `nextBatch()` or `close()`.
  Do not close or mutate them. To keep data across batch advances, explicitly
  retain/copy buffers and release retained data before closing the result. To
  outlive the result, copy into an independently owned allocator. Do not keep a
  batch object as a snapshot.
- `batch()` fails before the first successful read, after EOF, or after close.
  Repeated reads at EOF return false. Close even after EOF.
- `queryArrow` retains a pooled connection lease until the result closes. Borrowed
  connections remain open and uncommitted. Reader/export/schema failures clean up
  acquired resources; failures while advancing a graph result also return its lease.
  Close is idempotent and attempts all cleanup even if one resource's close fails.
- A stream and session are single-consumer objects, not thread-safe. Batch size is
  passed to the driver as a request, not a universal memory or row-count guarantee.
- Allocator accounting does not bound all native database memory. Configure the
  database's own memory limits separately. This API promises columnar hand-off,
  not universally zero-copy or a measured speedup.

## Row compatibility and Gremlin

`ArrowResult.rows()` transfers iteration/close ownership to a row view. Do not mix
row and batch iteration. It reads the vectors and converts values only on `get()`;
strings become Java `String`, while nested/temporal values follow Arrow Java's
representations rather than JDBC `Array`/`Struct` conventions. Dictionary-encoded
results must be consumed as batches using `dictionaries()`; the row convenience
currently rejects them explicitly.

With `JdbcEngine.withArrow(...)`, existing `graph.query(...)` calls and the optional
Gremlin adapter use this Arrow-backed row view. Without an exporter, the JDBC row
API remains available; requesting Arrow fails explicitly. The Gremlin adapter
still buffers its bounded scalar results and is not a streaming batch API.

## Verification

`ArrowIntegrationTest` checks actual DuckDB exports through the JNI compiler,
multi-batch and empty results, exact decimal/binary/timestamp/list values, nulls,
transaction ownership, partial close, pool lease return, schema/read/export/close
failures, and session cleanup. A guarded JDBC proxy rejects `next()` and
`getObject()` on the Arrow path. The optional Gremlin tests also execute through
an Arrow-configured engine. Run `make -C ~/orchiddb java-test` for both modules
and checks that DuckDB has not entered their production dependency graphs.
