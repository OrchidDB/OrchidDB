# Rust API

Find the public engine methods, result types, mapping builders, and planner entry points.

## GraphEngine

Import from `new_graph::engine`. Query methods are asynchronous and return `Result<QueryResult, String>`; lifecycle methods are synchronous.

| Method | Purpose |
| --- | --- |
| `GraphEngine::open(path)` | Open a persistent managed graph. |
| `GraphEngine::in_memory()` | Create an ephemeral managed graph. |
| `cypher(query).await` | Execute a Cypher statement. |
| `cypher_with_params(query, &params).await` | Execute with typed parameters. |
| `gremlin(query).await` | Execute a Gremlin traversal. |
| `sparql(query, ontology).await` | Execute with an owned `OntologyMapping`. |
| `execute_plan(&plan).await` | Execute a prepared `GraphPlan`. |
| `begin()` / `commit()` / `rollback()` | Manage a transaction. |
| `in_transaction()` | Inspect transaction state. |
| `checkpoint()` | Compact persisted graph records. |
| `replace_graph(graph)` | Install a replacement `PropertyGraph`. |
| `set_read_mode(mode)` | Select `Hybrid` or `SqlOnly`. |
| `set_sql_timeout(duration)` | Set the DuckDB SQL timeout. |

`QueryResult` contains `returned: ReturnedBatches`, `backend: ExecutionBackend`, and `stats: ExecStats`.

## MappedGraphEngine

Import from `new_graph::mapped_engine`. Construct with `MappedGraphEngine::new(executor, Arc::new(mapping))`.

| Method | Purpose |
| --- | --- |
| `cypher(query).await` | Query mapped tables through SQL. |
| `cypher_with_params(query, &params).await` | Read with typed Cypher parameters. |
| `gremlin(query).await` | Traverse mapped tables. |
| `sparql(query, ontology).await` | Read with a property-graph ontology. |
| `explain_cypher(query).await` | Return generated read SQL. |
| `cypher_update(query).await` | Execute a property assignment and return `usize` affected rows. |
| `cypher_update_with_params(query, &params).await` | Bind parameters to a property update. |
| `execute_sql(sql)` | Execute relational SQL in the session. |
| `executor()` / `executor_mut()` | Access the DuckDB executor. |
| `mapping()` | Inspect the graph mapping. |

Read methods return `Result<ReturnedBatches, String>`. The [mapped tutorial](mapped-graphs.md) demonstrates source schema registration and engine construction.

## RdfGraphEngine

Import from `new_graph::rdf_engine`. Construct with `RdfGraphEngine::new(executor, Arc::new(mapping), dataset_name)`.

| Method | Purpose |
| --- | --- |
| `sparql(query).await` | Query the selected RDF dataset. |
| `dataset()` | Return the configured dataset name. |
| `mapping()` | Inspect the RDF dataset mapping. |
| `into_executor()` | Consume the engine and recover its DuckDB executor. |

The [RDF dataset guide](rdf.md) shows table registration and graph-column configuration.

## Data and mapping types

| Module | Types |
| --- | --- |
| `ir::rel::mapping` | `GraphMapping`, `NodeMapping`, `EdgeMapping`, `MappedSource` |
| `ir::rel::rdf` | `RdfDatasetMapping`, `IriQuadSource` |
| `language::sparql` | `OntologyMapping`, `ClassMapping`, `PredicateMapping` |
| `ir::interpreter` | `ReturnedBatches` |
| `ir::catalog` | `PropertyGraph` |
| `ir` | `Value` |

## Planning and execution APIs

| Module | Entry points |
| --- | --- |
| `language::cypher` | `parse_query`, `CypherPlanner` |
| `language::gremlin` | `parse_traversal`, `GremlinPlanner` |
| `language::sparql` | `parse_query`, `SparqlPlanner` |
| `ir::plan` | `GraphPlan`, graph `Node` operators |
| `ir::rel` | `RelBackend`, `RelBackendOptions`, `LoweredPlan` |
| `ir::rel::sql` | `DuckDbExecutor`, `SqlExecutor`, `SqlDialect` |
| `ir::exec` | `IslandTarget`, `DataFusionTarget`, `SqlTarget`, `ExecStats` |

## Generate symbol documentation

Build the crate's Rust documentation locally to browse complete signatures and type definitions:

```sh
cargo doc --locked --no-deps
```

Open `target/doc/new_graph/index.html` in a browser. Enable optional features in the Cargo command when documenting their types.

## Error handling

Engine methods use `Result` so the application can handle failures at each boundary. Add application context when logging a failure, preserve the engine's error message, and roll back an explicit transaction before retrying conflicting work. See [transactions](transactions.md) for the control flow.
