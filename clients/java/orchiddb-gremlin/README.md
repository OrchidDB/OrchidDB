# Optional Gremlin module

Use TinkerPop's native Java fluent traversal API with OrchidDB compiling and executing SQL on your application's existing engine. No Gremlin Server, network connection, embedded graph store, or DuckDB driver is added by this module.

This is a first, read-only adapter for **scalar results**, not a full TinkerPop graph provider. It does not inherit the full engine's Gremlin conformance claim.

## Build and try

Requires the same Java 17+, Rust, and shared monorepo checkout as the base library. From `clients/java/`:

```sh
./scripts/build-gremlin.sh
./scripts/run-gremlin-example.sh
```

On macOS, set `JAVA_HOME=$(/usr/libexec/java_home -v 17)` if the shell selects an older JDK. The example creates tables in a caller-owned DuckDB connection, maps them, and prints `Ada knows [Grace]` and `People: 2`. It then confirms the connection remains open.

The base library builds independently with `./scripts/build.sh`. The adapter inherits the shared Maven parent and is included in reactor builds with `-Pgremlin`; it adds TinkerPop only when explicitly selected. For a local source build, install the adapter with `mvn -Pgremlin install` and use:

```xml
<dependency>
  <groupId>com.orchiddb</groupId>
  <artifactId>orchiddb-gremlin</artifactId>
  <version>0.2.1</version>
</dependency>
```

Your application separately supplies its JDBC driver. DuckDB and TinkerGraph appear only as test dependencies here. The adapter targets TinkerPop 3.7.7; TinkerPop 4 is not supported.

## Use

Create an `OrchidDB.Graph` with your mappings and caller-owned engine as described in the [base README](../README.md), then:

```java
import io.orchiddb.gremlin.OrchidGremlin;
import org.apache.tinkerpop.gremlin.process.traversal.P;

try (var g = OrchidGremlin.traversal(graph)) {
    var friends = g.V()
        .hasLabel("Person")
        .has("name", "Ada")
        .out("KNOWS")
        .<String>values("name")
        .toList();

    long adults = g.V().has("age", P.gte(18)).count().next();
}
```

See the [complete runnable example](src/test/java/io/orchiddb/gremlin/GremlinExample.java).

Traversal submission uses TinkerPop's `RemoteConnection` extension point **in process**. Structured bytecode is validated and serialized as literal-safe Gremlin for the existing Rust parser; it is never evaluated as Groovy and never uses `Traversal.toString()`. The Rust compiler generates SQL, and the existing engine session executes it. Your mappings, plan cache, UDF declarations, and JDBC connection configuration still apply.

## Supported surface

- Initial `V()` / `E()`, optionally with signed integer IDs.
- `has` with string keys and literal or `P` values, `hasLabel`, `hasId`, `hasNot`.
- `out`, `in`, `both`, `outE`, `inE`, `bothE`, `inV`, `outV`, `bothV`.
- Scalar projection: `values("key")` (one key), `id`, `label`, `count`.
- `is`, `dedup()` without labels, `identity`, `order` with one `by` modulator using a property key and/or `Order.asc` / `Order.desc`.
- `limit`, `skip`, `range` with nonnegative bounds.
- Anonymous traversal filters: `filter`, `not`, `and`, `or`, `where` (no label-based overloads).
- Predicates: `eq`, `neq`, `lt`, `lte`, `gt`, `gte`, `within`, `without`, and composed `and` / `or`.

Every top-level traversal must project a scalar before returning. The SQL compiler can still reject combinations requiring runtime-only behavior. Returned scalar values use the mapped JDBC column types: choose the matching Java generic type (for example `Long` for a BIGINT property).

Unsupported operations fail explicitly, including vertex/edge/property/path objects, maps/lists as results, multi-key `values`, mutations, side effects, sacks, arbitrary Java lambdas/comparators, custom predicates, and traversal source options/strategies. Gremlin bindings, arbitrary Java objects, date/decimal arguments and nonfinite numeric arguments are not serialized. Use standard anonymous traversals and supported scalar literals. Transactions remain under the caller's JDBC control; `g.tx()` is unsupported.

## Resource ownership and limits

Execution starts when the traversal is consumed. Submission executes synchronously on the calling thread, including the `submitAsync` extension point; its completed future does not imply background JDBC work. Each source serializes submissions. The underlying engine retains its existing concurrency rules.

This first adapter buffers results and releases the statement/result/engine lease **before returning values to TinkerPop**. This makes `.next()`, partial iteration, errors, and abandoned traversals safe without relying on TinkerPop to close a remote cursor. Closing `g` prevents new submissions but never closes your JDBC connection or DataSource, commits, rolls back, or changes connection settings. Already buffered results remain readable.

The default limit is 100,000 result rows. A traversal exceeding it fails; it never silently truncates. Set a different limit with `OrchidGremlin.traversal(graph, 10_000)` or constrain the query with `limit`/`range`. The limit counts rows, not bytes; very large scalar values still consume memory. True cursor streaming is future work.

Duplicate SQL rows are preserved as distinct occurrences (traverser bulk 1); count and dedup are executed by OrchidDB. No client-side substitute graph is built. This adapter uses the single-engine `Graph` API. Mixed-engine requests use the separate `FederatedQuery` JSON API.

## Verification and design references

Integration tests compare real DuckDB results with TinkerGraph for traversals, filters, predicates, ordering, IDs, duplicate counts and escaping. They also check partial consumption, result limits, connection ownership, transaction preservation and rejected operations.

- [TinkerPop RemoteConnection](https://github.com/apache/tinkerpop/blob/3.7.7/gremlin-core/src/main/java/org/apache/tinkerpop/gremlin/process/remote/RemoteConnection.java)
- [TinkerPop RemoteStep](https://github.com/apache/tinkerpop/blob/3.7.7/gremlin-core/src/main/java/org/apache/tinkerpop/gremlin/process/remote/traversal/step/map/RemoteStep.java)
