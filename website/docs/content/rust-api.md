# Rust API

Find the public engine methods, result types, mapping builders, and planner entry points.

## SQL generation

Use `compiler::compile` or `compiler::compile_json` to return SQL without a database driver. The optional `execution::SqlSession` interface accepts caller-owned sessions and result types. See [SQL compilation](sql-compiler.md) for runnable examples.

The engine APIs below require `features = ["duckdb"]`.

## GraphEngine

Import from `orchiddb::engine`. Cypher, Gremlin, ontology SPARQL, and `sparql_dataset` methods are asynchronous and return `Result<QueryResult, String>`; typed RDF queries return `SparqlResults`. Lifecycle methods are synchronous.

| Method | Purpose |
| --- | --- |
| `GraphEngine::open(path)` | Open a persistent managed graph. |
| `GraphEngine::in_memory()` | Create an ephemeral managed graph. |
| `GraphEngine::mapped(connection, Arc::new(mapping))` | Use application tables for all three languages. |
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
`stats.logical_plan`, `stats.layout_selections`, and `stats.representation_selections`
show the chosen inputs. See [equivalent representations](representations.md) for
registering alternative collection, SQL, and materialized sources.

## MappedGraphEngine compatibility facade

New applications use `GraphEngine::mapped`. For existing callers, import from `orchiddb::mapped_engine` and construct with `MappedGraphEngine::new(executor, Arc::new(mapping))`.

| Method | Purpose |
| --- | --- |
| `cypher(query).await` | Execute mapped statements through the shared runtime. |
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

## SPARQL on GraphEngine

| Method | Purpose |
| --- | --- |
| `sparql_query(query, dataset).await` | Return typed SELECT, ASK, or CONSTRUCT/DESCRIBE results. |
| `sparql_dataset(query, dataset).await` | Return Arrow batches and execution statistics. |
| `sparql_update(query, dataset, base).await` | Apply mapped row updates in the shared transaction. |
| `sparql_sql(query, dataset).await` | Inspect generated SQL. |
| `into_executor()` | Recover the same DuckDB connection. |

`RdfGraphEngine` remains a delegating compatibility facade. New applications use
`GraphEngine::mapped` with RDF rules on `GraphMapping`. See [RDF mappings](rdf.md).

## Data and mapping types

| Module | Types |
| --- | --- |
| `ir::rel::mapping` | `GraphMapping`, `NodeMapping`, `EdgeMapping`, `MappedSource` |
| `ir::rel::layout` | `LogicalSource`, `TableLayout`, `PartitionSpec`, `PartitionStatistics` |
| `ir::rel::collection_source` | `CollectionSource` |
| `ir::rel::representation` | `RepresentationSource`, `Representation`, `RepresentationInput`, `RepresentationDecision` |
| `ir::rel::rdf_mapping` | `RdfMapping`, `RdfTermMapping` |
| `ir::rel::rdf` | `RdfDatasetMapping`, `IriQuadSource` |
| `language::sparql` | `OntologyMapping`, `ClassMapping`, `PredicateMapping` |
| `engine` | `GraphEngine`, `QueryResult`, `SparqlResults`, `RdfTermValue` |
| `ir::runtime` | `ReturnedBatches` |
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
| `ir::exec` | `ExecStats` |

Graph IR is lowered before execution. The shared execution runtime uses DataFusion
physical operators and native kernels; there is no standalone Graph IR
interpreter or legacy island target API.

## Generate symbol documentation

Build the crate's Rust documentation locally to browse complete signatures and type definitions:

```sh
cargo doc --locked --no-deps
```

Open `target/doc/orchiddb/index.html` in a browser. Enable optional features in the Cargo command when documenting their types.

## Error handling

Engine methods use `Result` so the application can handle failures at each boundary. Add application context when logging a failure, preserve the engine's error message, and roll back an explicit transaction before retrying conflicting work. See [transactions](transactions.md) for the control flow.
