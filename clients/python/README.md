# OrchidDB for Python

Source builds use the shared core in this monorepo. See [build and test instructions](../README.md).

Compile Cypher, Gremlin and SPARQL to SQL, execute through a caller-owned engine,
and consume Arrow batches. No DuckDB driver is bundled or imported by the library.
The compiler does not touch your data. Graph mapping and schema metadata are explicit.

```sh
python -m venv .venv
. .venv/bin/activate
python -m pip install -r examples/requirements.txt
python examples/people.py
```

The example installs `orchiddb[arrow]==0.3.0` from PyPI and its application-owned DuckDB driver. Platform wheels include the compiler for Linux ARM64/x86_64 and macOS ARM64/x86_64; no library path is needed.

For source development, run `make native` at the repository root and install
this client from `clients/python/`. The generated metadata matches the staged
library from the same checkout. Release wheels bundle their platform compiler.
No network downloads happen at import time. ABI and core revision are validated. An explicit development library may report
the pinned revision with a `-dirty` suffix; bundled release libraries require an exact match.

```python
from orchiddb import Compiler, DuckDBEngine, Graph
# connection is your existing DuckDB connection; see examples/people.py for mapping.
graph = Graph(Compiler(), DuckDBEngine(connection), tables=tables, nodes=nodes)
with graph.query_arrow("MATCH (p:Person) WHERE p.name=$name RETURN p.name AS name",
                       parameters={"name": "Ada"}, batch_size=65536) as reader:
    print(reader.schema)
    for batch in reader:
        consume(batch)  # pyarrow.RecordBatch; no row or JSON conversion
```

The reader closes on context exit, including exceptions. Retained PyArrow batches
own their buffers. The adapter never closes the borrowed connection, commits,
rolls back, configures extensions, or installs UDFs. Do not reuse that connection
until the reader is closed. Overlapping readers through the same adapter fail.
Register UDFs yourself and pass their compiler signatures via `functions`.

`graph.plan(query)` returns SQL without executing it. `Compiler.compile(request)`
accepts the shared v1 JSON-shaped request; use `language="gremlin"` or `"sparql"`
with `Graph` for those languages. SQL support is deliberately narrower than the
managed engine's conformance suite. Parameters are specialized into SQL; recompile
when values or metadata change. Unsupported queries raise `CompilationError`.

Implement the `ArrowEngine` protocol for another backend: provide `dialect` and a
context manager yielding a `pyarrow.RecordBatchReader`. `PostgresEngine` and
`query_federated` support PostgreSQL and mixed DuckDB/PostgreSQL execution. Caller controls
connections, Arrow allocation, caches and transaction boundaries.

## Releases

CI builds pinned native code and runs real Arrow/DuckDB tests. The manual release
workflow requires a matching `v0.3.0` tag, builds platform wheels, and publishes to
PyPI via trusted publishing. Configure PyPI's `orchiddb` trusted publisher for this
repository, workflow `release.yml`, environment `pypi`. Version 0.3.0 is published on PyPI.

Licensed under [the OrchidDB GPL-3.0-only license](LICENSE.md).

DuckDB Arrow export follows its [Python Arrow API](https://duckdb.org/docs/current/guides/python/export_arrow).
Arrow batching does not itself guarantee that an engine streams query execution;
execution buffering and cancellation remain backend-specific.

RDF vocabulary can reference the same application tables: pass `rdf=[...]` and
optionally `dataset="default"` to `Graph`, then use `language="sparql"`. Each rule
contains `table`, `subject`, `predicate`, and `object` term mappings. For example,
`{"kind":"template","prefix":"urn:person:","columns":["id"]}` constructs a
subject and `{"kind":"literal","column":"name"}` exposes a property. These
rules require a native compiler built with the shared RDF mapping API.

## Generate statistics once

```python
report = graph.generate_statistics()  # bounded reads on graph.engine's session
plan = graph.plan("MATCH (p:Person) WHERE p.name = 'Ada' RETURN p.name")
print(plan.diagnostics)                # estimates and physical-source choices
print(report["report"])               # collection coverage and any skipped work
graph.save_statistics("statistics.json")
graph.clear_statistics()               # subsequent plans use no generated statistics
graph.load_statistics("statistics.json")
```

Generation is optional and explicit. Shared Rust code decides what to collect
and calculates the statistics. Python forwards bounded Arrow IPC batches; it
does not implement an estimator. The retained native catalog is automatically
used by later compilations, including Cypher, Gremlin, and SPARQL. Regeneration
replaces it only after finishing; interruption preserves the previous catalog.
`Compiler.close()` releases the retained catalog and never closes your database.

`DuckDBEngine` uses the caller's connection and transaction, closes result
readers, and interrupts requests at their deadline. Custom adapters can provide
`statistics_arrow(request)` as a context manager yielding Arrow batches, while
enforcing `timeout_ms`, `max_rows`, and `max_bytes` during acquisition/reading.
Without a bounded adapter, collection reports skipped work. Application-owned
collection drivers can also call `Compiler.statistics_command(command)` directly.
No background refresh, profiles, or tuning tiers are involved. The snapshot
records its mapping identity; regenerate after changing mappings or source data.
Compilation does not read the database.

`Graph` accepts `logical_sources`, `collection_sources`, and
`representation_sources` alongside existing mappings. Full native plan fields
are retained in `CompiledQuery.diagnostics`. A compiler retains one catalog;
use a separate compiler instance for each independently analyzed mapping.

## Permission pushdown

Pass permission scopes on node mappings and a principal on each protected query. The helpers build provider-neutral request data; the relation must contain effective grants for that principal.

```python
from orchiddb import Authorization, PermissionRelation, PermissionScope

grant_source = "effective_grants"
nodes = [{
    "label": "Document", "table": "documents", "id": "id",
    "properties": {"title": "title", "project_id": "project_id"},
    "permission_scopes": [
        PermissionScope("id", PermissionRelation.flat(grant_source, "document", "view")).to_dict(),
        PermissionScope("project_id", PermissionRelation.flat(grant_source, "project", "view")).to_dict(),
    ],
}]
graph = Graph(compiler, engine, tables=tables, nodes=nodes)
with graph.query_arrow("MATCH (d:Document) RETURN d.title", authorization=Authorization("user", "alice")) as reader:
    result = reader.read_all()
```

### Quickwit and Elasticsearch

Register a remote engine alongside your existing SQL connection and call
`query_federated` with the same engine names as your mapping. Search relationships
and remote tables use the shared native HTTP transport; dependent SQL joins still
run on your SQL connection.

```python
from orchiddb import Compiler, DuckDBEngine, RemoteEngine, query_federated

with Compiler() as compiler:
    with RemoteEngine(compiler, "quickwit", "http://localhost:7280",
                      page_size=1000) as search:
        engines = {"db": DuckDBEngine(connection), "search": search}
        with query_federated(compiler, request, engines) as reader:
            table = reader.read_all()
```

Use `"elasticsearch"` and its endpoint for Elasticsearch. `request` contains your
engine registry, tables, graph mappings, and query; see the
[search engine guide](https://docs.orchiddb.com/remote-engines.html) for
complete mappings and queries. Install the `arrow` extra. Credentials belong in
`RemoteEngine(..., authentication={"type": "bearer", "token": token})`, not in the
query mapping. Basic authentication and API keys are supported too. Sessions are
caller-owned: federation closes result readers but never your SQL connection or
remote session. HTTP failures and incomplete results raise errors. Calls are
synchronous; use a worker thread in an asynchronous application.

Custom remote adapters can implement `dialect` and
`execute_requests(requests, columns)` as a context manager yielding an Arrow
reader. This retains nested types, large integers, and JSON/SQL null distinctions.
For live integration tests, set `ORCHIDDB_REMOTE_FIXTURE` to the shared release
fixture JSON and run `pytest tests/test_remote_engines.py`.
