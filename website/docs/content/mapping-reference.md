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
.with_id("order_id")
.property("total", "total")
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
edge_id = "order_id"

[edge.ORDERED.properties]
total = "total"
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
