# OrchidDB for Elixir

Source builds use the shared core in this monorepo. See [build and test instructions](../README.md).

Compile graph queries to SQL with the shared database-free compiler. Optionally
execute on a caller-owned ADBC connection, exposing the native **Arrow C Stream**
to a callback. No DuckDB or other engine is bundled. Query results stay in the
application without row or IPC conversion. Explicit statistics generation sends
bounded collection batches to the shared core through the NIF.

```elixir
# mix.exs
{:orchiddb, "~> 0.3.0"}
# Add {:adbc, "~> 0.12"} when using the Arrow execution convenience function.
```

Version 0.3.0 is published on Hex. Run the standalone example against that package:

```sh
cd examples
mix deps.get
export ORCHIDDB_NATIVE_LIBRARY=/absolute/path/liborchiddb_compiler.dylib
mix run compile.exs
mix run duckdb.exs
```

The small C NIF builds with your system compiler (`make`, Erlang headers and `cc`).
Run `make native` at the repository root and point `ORCHIDDB_NATIVE_LIBRARY` at
`target/debug/liborchiddb_compiler.dylib` (`.so` on Linux). The generated
`CORE_REVISION` matches that library. No compiler binary is downloaded implicitly.
The compiler uses a dirty CPU scheduler and its own Rust runtime.
One shared library remains loaded for the VM lifetime; switching it requires a VM restart.

```elixir
request = %{
  version: 1, dialect: "duckdb", language: "cypher",
  query: "MATCH (p:Person) RETURN p.name AS name",
  tables: [%{name: "people", columns: [
    %{name: "id", data_type: "int64"}, %{name: "name", data_type: "string"}]}],
  nodes: [%{label: "Person", table: "people", id: "id", properties: %{name: "name"}}]
}
{:ok, plan} = OrchidDB.compile(request)
# conn and destination are your configured ADBC connections.
OrchidDB.query_arrow(conn, request, fn arrow_stream ->
  Adbc.Connection.bulk_insert!(destination, arrow_stream, table: "result")
end)
```

The stream can be consumed once and is valid **only inside the callback**.
Do not return or retain its pointer. ADBC owns statement/stream cleanup; OrchidDB
never stops your connection or database process or alters transactions. Register
extensions/UDFs yourself, declaring matching function signatures in the request.
See [ADBC query_pointer](https://adbc.hexdocs.pm/Adbc.Connection.html#query_pointer/5).

Requests support `cypher`, `gremlin`, and `sparql`; mapping, schema, ontology and
function signatures follow core's JSON v1 compiler contract. DuckDB and PostgreSQL
SQL rendering and mixed-engine execution through `query_federated` are supported.
Gremlin text compiles to SQL; there is no native Gremlin executor.
Parameters are literals specialized into SQL; compile again after changing them.
Errors return `{:error, reason}`. The integration suite uses the real DuckDB ADBC driver to execute compiled graph
queries and ingest Arrow C Streams directly. It verifies int64 values, nulls,
caller transactions/rollback, connection reuse, and consumer-error cleanup.
Run `mix test` to download the DuckDB test driver and exercise the complete path.

## Permission pushdown

Add a flat effective-grants table to the request's `tables`, then attach one or more scopes to a node. Each scope matches a relation resource ID against a node source column; all scopes must contain effective grants for the query principal.

```elixir
alias OrchidDB.Permission

grants = Permission.relation("effective_grants", "document", "view")
project_grants = Permission.relation("effective_grants", "project", "view")
request = Map.merge(request, %{
  authorization: Permission.authorization("user", "alice"),
  nodes: [%{label: "Document", table: "documents", id: "id",
    properties: %{title: "title", project_id: "project_id"},
    permission_scopes: [
      Permission.scope("id", grants),
      Permission.scope("project_id", project_grants)
    ]}]
})
{:ok, plan} = OrchidDB.compile(request)
```

Use `Permission.relation/4` to override relation column names. The compiler fails closed when scopes have no principal; it does not connect to or synchronize an external permission system.

## Release

`mix hex.build` creates the source package including the C NIF. Workflow
`release.yml` validates the matching version tag and publishes with the repository
secret `HEX_API_KEY` in environment `hex`. Hex package `orchiddb` version 0.3.0 is published. Compiler binaries are independently
built from the shared core in this repository.

[GPL-3.0-only license](LICENSE.md).

## One-time statistics

```elixir
{:ok, statistics} = OrchidDB.Statistics.generate(request, fn work ->
  MySession.collect_bounded(connection, work)
end)
{:ok, plan} = OrchidDB.compile(request, statistics: statistics)
:ok = OrchidDB.Statistics.save(statistics, "statistics.json")
{:ok, _} = OrchidDB.Statistics.clear(statistics)
{:ok, statistics} = OrchidDB.Statistics.load("statistics.json")
```

The callback executes collection SQL on the application's existing session. It
must enforce the supplied `timeout_ms`, `max_rows`, and `max_bytes`, returning
`{:ok, %{"rows" => rows}}` or `{:ok, %{"ipc" => base64_arrow_stream}}`. Return
`{:error, reason}` if bounded execution is unavailable. ADBC's current Elixir API
does not expose query interruption, so this client does not claim that a BEAM
call timeout bounds database execution; use an adapter with genuine cancellation
or a server-side statement deadline.

Generation may make several calls. Shared Rust code chooses requests and computes
statistics; the Elixir client only transports them. `statistics.report` records
coverage and skipped sources. A retained native catalog is reused by compilation,
which preserves all diagnostics and never reads data or refreshes automatically.
Pass `statistics: statistics` to `query_arrow` as well. Save/load persists portable
snapshots rather than process-local handles. Release with `clear` after the last
user finishes. To regenerate, obtain a replacement successfully before clearing
the old handle. `OrchidDB.statistics_command/2` exposes the shared protocol for
batch submission and explicit cancellation.

Collectors must set `truncated: true` when transport limits stop a response before
EOF; the coordinator retains those observations as a partial sample. It must not
infer a complete source row count from a shortened response.

## Quickwit and Elasticsearch

```cypher
MATCH (q:Question)-[match:SIMILAR_TO]->(d:Document)
RETURN q.id, d.title, match.score
ORDER BY match.score DESC
```

Define the relationship using `text.bm25(source.body, target.body)` and a
per-source limit. In the compiler request, register `text` with dialect
`quickwit` or `elasticsearch`, alongside your SQL execution engine. The
[search mapping documentation](https://docs.orchiddb.com/remote-engines.html)
shows remote-owned documents and remote indexes over authoritative SQL tables.

```elixir
{:ok, text} = OrchidDB.RemoteEngine.start_link("quickwit", %{
  endpoint: "https://quickwit.example.com",
  authentication: %{type: "bearer", token: token},
  page_size: 1000,
  request_timeout_ms: 30_000
})
engines = %{
  "local" => %{dialect: "duckdb", query: fn sql, consume ->
    Adbc.Connection.query_pointer(connection, sql, consume)
  end},
  "text" => OrchidDB.RemoteEngine.registry(text)
}
try do
  OrchidDB.query_federated(engines, request, fn stream ->
    stream |> Adbc.StreamResult.to_ipc_stream() |> Adbc.Result.from_ipc_stream!()
  end)
after
  GenServer.stop(text)
end
```

Use `"elasticsearch"` for Elasticsearch. The native compiler library must include
the selected optional engine. `RemoteEngine` owns only its HTTP session; the
application owns SQL connections. `close/1` releases the native session and is
idempotent; stopping the process releases it as well. Call
`clear_metadata_cache/1` after changing index mappings. Authentication, request
timeouts, batching, pagination, and response limits use the shared native
transport. Partial results and transport failures propagate as errors.

For other request engines, a registry entry can supply
`:execute_requests`, a function accepting bound request payloads and declared
columns. Return `{:ok, %{ipc: base64_arrow_stream}}` or
`{:ok, %{rows: typed_rows}}`. JSON-domain cells use serialized JSON text;
SQL-null cells use `nil`; decimal strings preserve wide numeric values.
Ordinary SQL entries retain `:query` and optional `:exchange_data` callbacks.
Dependent SQL operations and remote operations preserve typed results across
subsequent SQL islands. `execute_federated/4` accepts an already compiled plan.

`mix test` covers dependent SQL, empty inputs, and connection ownership. Set
`ORCHIDDB_REMOTE_FIXTURE` to a live fixture JSON file to additionally test both
HTTP engines, pagination, correlated BM25, authoritative SQL joins, JSON
properties, and session lifecycle. Runtime tests run locally, never in CI.
