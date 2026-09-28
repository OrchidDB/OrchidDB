# Transactions and storage

Group graph changes, control commit boundaries, and maintain a persistent managed graph.

## Explicit transactions

`GraphEngine` provides `begin`, `commit`, `rollback`, and `in_transaction`. Queries inside a transaction see earlier writes from that transaction.

```rust
graph.begin()?;
let write = graph.cypher(
    "CREATE (:Person {name:'Alice'})"
).await;
match write {
    Ok(_) => graph.commit()?,
    Err(error) => {
        graph.rollback()?;
        return Err(error);
    }
}
```

For several statements, keep them within the same transaction and commit after all application checks succeed. Dropping an engine with an active transaction discards its uncommitted work.

## Statement atomicity

A failed graph statement restores its prior graph state. Writes executed outside an explicit transaction have an atomic commit boundary of their own. This makes individual operations straightforward while allowing applications to group related work explicitly.

## Multiple engine instances

Separate engines use DuckDB snapshot isolation. An engine outside a transaction refreshes its committed snapshot when it queries. Within a transaction, the snapshot provides a consistent view together with that transaction's writes.

If writers conflict, roll back the failed transaction and retry the complete unit of work against a fresh snapshot. Apply application-level retry limits and ensure that external side effects are coordinated with successful commits.

## Persistence and checkpoints

Managed writes persist changed node, edge, and label records transactionally. A checkpoint compacts those records into a graph snapshot:

```rust
graph.checkpoint()?;
```

A checkpoint participates in an active transaction, so a rollback also rolls back the compaction. `replace_graph` installs a new graph checkpoint as part of its replacement operation.

## Mapped engine transactions

`GraphEngine::mapped` uses the same `begin`, `commit`, and `rollback` API for
Cypher, Gremlin, and SPARQL. All statements share one DuckDB connection. Reads
see earlier writes from any of these languages in the transaction.

```rust
graph.begin()?;
let changed = graph.cypher(
    "MATCH (p:Person) WHERE p.age >= 30 SET p.age = p.age + 1 RETURN p.age"
).await;
match changed {
    Ok(result) => {
        graph.commit()?;
        println!("returned {} rows", result.returned.batch.num_rows());
    }
    Err(error) => {
        graph.rollback()?;
        return Err(error);
    }
}
```

Outside an explicit transaction, mapped statements commit atomically or roll
back on failure/cancellation. A failed mapped execution inside an explicit
transaction marks it failed; call `rollback` before continuing. Do not assume
that only the failing statement was undone and then commit earlier work.

Existing `MappedGraphEngine` callers retain `executor_mut().begin()`, `commit()`,
and `rollback()` on the compatibility facade. These are not methods on
`GraphEngine`. For typed RDF reads use `sparql_query`; for updates use
`sparql_update(update, dataset, base)`. Neither needs a second RDF engine.

## Back up a graph file

For a simple file backup workflow, stop writers, close connections to the database, and then copy the database file. Keep the backup together with the application revision and mapping configuration needed to interpret it. Test restores by opening a copied database and running representative reads.

## Upgrading a local database

Stop other processes using the file and keep a backup before upgrading. On open,
OrchidDB recognizes a prior graph-table namespace by its reserved schema and
validates the checkpoint before renaming tables in a transaction. Checkpoints
and incremental records are preserved. Ambiguous or corrupt stores are rejected
instead of being replaced with an empty graph. Older typed Arrow metadata is
accepted while new metadata uses the OrchidDB namespace.

Rebuild JVM artifacts together with the native `orchiddb-jvm-store` executable;
Java packages, configuration keys, and environment variables now use OrchidDB.
