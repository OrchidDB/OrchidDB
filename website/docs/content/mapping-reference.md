# Mapping reference

This chapter documents the optional core runtime APIs. For compiler-only clients with caller-owned engines, start with [Client APIs](client-apis.md) and [SQL compilation](sql-compiler.md).


Define graph labels, relationship types, identities, and properties with Rust builders or TOML.

## Node mappings

`NodeMapping::table(label, table, id_column)` maps a label to a table or view. Add properties with `.property(graph_name, column_name)`.

```rust
NodeMapping::table("Person", "customers", "customer_id")
    .property("name", "full_name")
    .property("age", "age")
```

Use stable IDs within each label. The graph label and identity together distinguish elements. Property names are the vocabulary used in queries; column names belong to your physical schema.

## Relationship mappings

The edge constructor takes the relationship type, source table, source ID column, destination ID column, source label, and destination label:

```rust
EdgeMapping::table(
    "ORDERED", "orders", "customer_id", "order_id", "Person", "Order"
)
.foreign_key(ForeignKeyEndpoint::Destination)
```

One physical table can supply both nodes and relationships. In this example, an order is a node, and its `customer_id` also defines a relationship from a person to that order.

Use a join table for a many-to-many relationship:

```rust
EdgeMapping::table(
    "FOLLOWS", "follows", "src", "dst", "Person", "Person"
)
```

## Register source schemas

`GraphMapping::register_table_schema` accepts an Arrow `SchemaRef`. Describe physical column names, data types, and nullability. Use `register_table` when you already have a DataFusion table provider.

| Builder or method | Purpose |
| --- | --- |
| `GraphMapping::new()` | Create an empty graph mapping. |
| `register_table_schema(name, schema)` | Register schema metadata for SQL planning. |
| `register_table(name, provider)` | Register a DataFusion table provider. |
| `register_view(name, sql)` | Register a SQL-defined view in the mapping. |
| `map_node(mapping)` | Add a label mapping. |
| `map_edge(mapping)` | Add a relationship mapping. |
| `labels()` / `rel_types()` | Inspect the mapped vocabulary. |
| `physical_table_names()` | Get physical source names for SQL preparation. |

## TOML format

```toml
[node.Person]
table = "users"
id = "id"

[node.Person.properties]
name = "name"
age = "age"

[node.Order]
table = "orders"
id = "order_id"

[node.Order.properties]
total = "total"

[edge.ORDERED]
table = "orders"
src = "user_id"
dst = "order_id"
src_label = "Person"
dst_label = "Order"
foreign_key = "dst"
```

Load it with `GraphMapping::from_toml(text)` and serialize with `mapping.to_toml()`. Register the physical table schemas after loading. TOML contains the mapping definition; table providers are registered by the application.

For a query-backed source, use a `query` key in place of `table`:

```toml
[node.Adult]
query = "SELECT id, name, age FROM users WHERE age >= 18"
id = "id"

[node.Adult.properties]
name = "name"
age = "age"
```

## Design a mapping

Choose names that match your application domain. Keep node IDs stable, match endpoint column types to their node ID types, and expose the properties that queries should use. Use `.with_id(...)` for table-backed relationships that participate in [property updates](updates.md).

For SQL-defined labels and reusable physical views, continue to [views and SQL sources](views.md).

## Writable one-to-many relationships

Use `.foreign_key(ForeignKeyEndpoint::Destination)` when the destination node's
row holds the reference to the source node. Use `Source` for the reverse graph
direction. This is explicit: an ordinary table-backed edge continues to own its
own row.

```rust
use orchiddb::ir::rel::mapping::{EdgeMapping, ForeignKeyEndpoint, NodeMapping};

mapping.map_node(NodeMapping::table("Customer", "customers", "id")
    .property("id", "id"));
mapping.map_node(NodeMapping::table("Order", "orders", "id")
    .property("id", "id")
    .property("total", "total"));
mapping.map_edge(EdgeMapping::table(
    "HAS_ORDER", "orders", "customer_id", "id", "Customer", "Order"
).foreign_key(ForeignKeyEndpoint::Destination));
```

The existing schema can enforce the relationship:

```sql
CREATE TABLE customers (id BIGINT PRIMARY KEY);
CREATE TABLE orders (
    id BIGINT PRIMARY KEY,
    customer_id BIGINT NOT NULL REFERENCES customers(id),
    total DOUBLE NOT NULL
);
```

Create both the child and the required relationship in one statement:

```cypher
MATCH (c:Customer {id: 42})
CREATE (c)-[:HAS_ORDER]->(o:Order {id: 101, total: 29.95})
RETURN o
```

The runtime combines the node properties and the relationship's FK assignment
into one `INSERT INTO orders (id, customer_id, total) ...`. It creates no other
table. An edge to an existing child updates that row. Edge identities are the
child primary keys, qualified by relationship type; do not supply independent
relationship IDs. A child can have at most one live edge per FK mapping.

Equivalent TOML:

```toml
[edge.HAS_ORDER]
table = "orders"
src = "customer_id"
dst = "id"
src_label = "Customer"
dst_label = "Order"
foreign_key = "dst"
```

`edge_id` is derived from the child endpoint and may be omitted. If supplied, it
must be the child key column. `foreign_key = "src"` selects the source endpoint
as the child.

Deleting the relationship sets the FK to NULL, preserving the child row and its
other columns. A required FK rejects this operation. To reparent a child, delete
the old relationship and create its replacement in the same statement; only the
final FK value is written. Deleting the child removes its FK relationship with
it (use the graph language's detach-delete operation where required). Deleting a
parent does not implicitly delete its children; remaining database constraints
apply.

A nullable FK with no value produces no graph edge in either SQL scans or native
traversals. Newly created child rows without a relationship receive NULL for the
FK, so a `NOT NULL` constraint fails; SQL defaults on that FK do not invent an
implicit relationship. Required properties and links must be complete within a
single statement, even in an explicit transaction.

Column write ownership must be unambiguous. Do not also expose the FK as a node
property, and do not map relationship property columns as node properties.
Several FK relationships may share a child table if each owns different columns.
The child and parent must be table-backed and use scalar keys with matching FK
and parent-key types. Composite keys, query-backed writes, and cyclic writes
requiring deferred constraints are not supported. Compatible rows are still
batched, with dependencies ordered before referencing rows.

DuckDB currently rejects deleting a referenced parent in the same transaction
that removes its child, even when the child is removed first. OrchidDB rolls
back the failed statement; commit the child removal before deleting the parent.
