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

For property updates over existing tables, control the DuckDB executor transaction:

```rust
graph.executor_mut().begin().map_err(|error| error.to_string())?;
let changed = graph.cypher_update(
    "MATCH (p:Person) WHERE p.age >= 30 SET p.age = p.age + 1"
).await;
match changed {
    Ok(count) => {
        graph.executor_mut().commit().map_err(|error| error.to_string())?;
        println!("updated {count} rows");
    }
    Err(error) => {
        graph.executor_mut().rollback().map_err(|error| error.to_string())?;
        return Err(error);
    }
}
```

The `graph` variable in this example is a `MappedGraphEngine`. Outside an explicit executor transaction, an update commits atomically.

## Back up a graph file

For a simple file backup workflow, stop writers, close connections to the database, and then copy the database file. Keep the backup together with the application revision and mapping configuration needed to interpret it. Test restores by opening a copied database and running representative reads.
