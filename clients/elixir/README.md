# OrchidDB for Elixir

Register schema on an existing ADBC connection, then execute query text:

```elixir
schema = "schema.json" |> File.read!() |> Jason.decode!()
{:ok, graph} = OrchidDB.connect(adbc_connection, schema)
{:ok, result} = OrchidDB.query(graph,
  "MATCH (p:Person) WHERE p.name=$name RETURN p.name AS name",
  parameters: %{name: "Ada"})
IO.inspect(Adbc.Result.to_map(result))
```

`OrchidDB.query_arrow(graph, text, callback, options)` passes a native Arrow C
stream into the callback. Consume it within the callback; do not retain the
pointer. The caller owns the ADBC connection and transaction. Set `language:`
on each query; schema/RDF mappings remain registered on the graph connection.
There is no public compile-only API or combined schema/query request.

Build from this checkout using a Mix path dependency on `clients/elixir`.
Run `make native` at the repository root and set `ORCHIDDB_NATIVE_LIBRARY` to
`target/debug/liborchiddb_compiler.dylib` (macOS) or `.so` (Linux). Then:

```sh
cd clients/elixir/examples
mix deps.get
mix run duckdb.exs
```

[The example](examples/duckdb.exs) creates its own database and demonstrates
Arrow transfer and connection ownership. PostgreSQL adapters can select
`dialect: "postgres"` when registering schema. `query_federated` accepts graph
query text with separately configured engine callbacks. Explicit statistics
generation accepts a bounded collector and returns a connection containing the
new retained statistics handle; database execution stays caller-owned. Use the
returned connection for subsequent queries. `save_statistics`, `load_statistics`,
and `clear_statistics` manage snapshots; `close` releases retained statistics
without closing the underlying database connection.

[Source builds and API migration](../README.md)
