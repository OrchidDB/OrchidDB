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
must be the complete ordered child key. `foreign_key = "src"` selects the source endpoint
as the child.

Deleting the relationship clears its nullable, non-primary-key FK components,
preserving the child row, shared key components, and other required columns.
An FK with no nullable non-key component rejects this operation. To reparent a child, delete
the old relationship and create its replacement in the same statement; only the
final FK value is written. Deleting the child removes its FK relationship with
it (use the graph language's detach-delete operation where required). Deleting a
parent does not implicitly delete its children; remaining database constraints
apply.

An FK with any NULL component produces no graph edge in either SQL scans or native
traversals. Newly created child rows without a relationship receive NULL for the
FK, so a `NOT NULL` constraint fails; SQL defaults on that FK do not invent an
implicit relationship. Required properties and links must be complete within a
single statement, even in an explicit transaction.

Column write ownership must be unambiguous. Shared primary-key columns may appear
in node properties and multiple FKs; they are immutable and all supplied values
must agree. Other FK columns and relationship property columns must not also be
node properties or owned by another relationship mapping.
The child and parent must be table-backed; each ordered FK component must match
its corresponding parent-key type. Query-backed writes and cyclic writes requiring
deferred constraints are not supported. Compatible rows are batched with
referenced rows inserted first. DuckDB constraints remain binding: in particular,
updating an indexed FK column on a row referenced by another table can be rejected
by DuckDB even when its primary key does not change. The engine propagates that
error and rolls back; it does not disable constraints or rewrite unrelated rows.


## Composite keys

Use ordered column arrays for node IDs, edge IDs, and either endpoint:

```rust,ignore
mapping.map_node(NodeMapping::table("Customer", "customers", ["tenant", "id"])
    .property("tenant", "tenant").property("id", "id").property("name", "name"));
mapping.map_node(NodeMapping::table("Order", "orders", ["tenant", "id"])
    .property("tenant", "tenant").property("id", "id").property("total", "total"));
mapping.map_edge(EdgeMapping::table(
    "HAS_ORDER", "orders", ["tenant", "customer_id"], ["tenant", "id"],
    "Customer", "Order",
).foreign_key(ForeignKeyEndpoint::Destination));
```

Here `orders(tenant, customer_id)` references `customers(tenant, id)`, while the
order's own primary key is `(tenant, id)`. No relationship table is created. The
shared tenant component cannot change when reparenting an existing order.

```toml
[node.Customer]
table = "customers"
id = ["tenant", "id"]
[node.Customer.properties]
tenant = "tenant"
id = "id"
name = "name"

[node.Order]
table = "orders"
id = ["tenant", "id"]
[node.Order.properties]
tenant = "tenant"
id = "id"
total = "total"

[edge.HAS_ORDER]
table = "orders"
src = ["tenant", "customer_id"]
dst = ["tenant", "id"]
src_label = "Customer"
dst_label = "Order"
foreign_key = "dst"
```

Columns match by position, not name. All primary-key components are required,
non-null scalars; nested keys and duplicate/empty column lists are rejected.
Existing scalar constructor arguments and serialized strings remain supported.
Rust callers accessing mapping fields directly now receive `KeyColumns`; use
`.columns()` to inspect the ordered names. Ordinary relationship tables can use
`.with_id(["tenant", "edge_id"])` and composite endpoints too.

Cypher supplies all key components through their declared property aliases:

```cypher
CREATE (:Customer {tenant:'acme', id:42, name:'Alice'})
       -[:HAS_ORDER]->(:Order {tenant:'acme', id:101, total:29.95})
```

Gremlin accepts and returns composite IDs as ordered lists:

```groovy
g.addV('Customer').property(T.id, ['acme', 42]).property('name', 'Alice')
g.V().hasLabel('Customer').hasId(eq(['acme', 42]))
```

Use `eq(tuple)` for one composite ID: Gremlin's ordinary `hasId(list)` overload
expands the list as multiple candidate IDs. The runtime keeps component types and
boundaries through joins, traversals, paths, and serialization. No implicit key
generation is introduced. If a child's FK consists entirely of its primary-key
columns, create its relationship explicitly in the same statement as the child.

A complete runnable example is `examples/composite_keys.rs`:
`cargo run --features duckdb --example composite_keys`.
