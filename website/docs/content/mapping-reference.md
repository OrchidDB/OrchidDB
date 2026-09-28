# Mapping reference

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
| `register_logical_source(source)` | Choose among equivalent physical table layouts. |
| `register_representation_source(source)` | Choose among equivalent derived relations and materialized tables. |
| `register_collection_source(source)` | Expand a list column as a named read-only relation. |
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

## Collection columns as logical tables

A collection source expands one native list column into a named, read-only relation. It is an inline logical plan, so no database view needs to be created. Node, edge, RDF, and query-backed mappings can reference its name.

### Mapping example

For `orders(id BIGINT, items STRUCT(item_id BIGINT, sku VARCHAR, quantity BIGINT)[])`:

```json
{
  "collection_sources": [{
    "name": "order_items",
    "table": "orders",
    "column": "items",
    "parent_columns": {"order_id": "id"},
    "fields": {"item_id": "item_id", "sku": "sku", "quantity": "quantity"}
  }],
  "nodes": [{
    "label": "Item",
    "table": "order_items",
    "id": ["order_id", "item_id"],
    "properties": {"order_id": "order_id", "sku": "sku", "quantity": "quantity"}
  }]
}
```

The parent column and field maps are **output alias → source column/field**. For a scalar list, replace `fields` with `"element": "value"`. Supply exactly one of `element` or nonempty `fields`.

The compiler's existing string schema declarations accept `list:string`, `list:int64`, and `list:struct:{"item_id":"int64","sku":"string","quantity":"int64"}`. The JSON object after `struct:` maps field names to recursive type strings. The Rust API accepts Arrow List, LargeList, or FixedSizeList providers directly.

```rust
mapping.register_collection_source(collection_source)?;
```

The type is `orchiddb::ir::rel::collection_source::CollectionSource`. Register the parent schemas/providers first, followed by partition-layout sources, then collection sources. A collection's parent can be a physical table, a logical source with equivalent physical layouts, or a representation source exposing a supported list column. Register each dependency before its dependent source. Replacing a parent provider refreshes dependent collection schemas; incompatible replacements leave the collection unbound instead of retaining stale data.

TOML round-trips definitions in `[collection_sources]` with `catalog` holding a JSON array string, like the layout catalog. Register physical providers after loading TOML.

### Plans and partition selection

```cypher
MATCH (i:Item)
WHERE i.order_id = 42 AND i.quantity > 1
RETURN i.sku, i.quantity
```

The plan contains the parent scan, list projection, `Unnest`, field projection, and remaining filters. These appear in `CompiledSql.logical_plan` and execution `stats.logical_plan`. Physical-layout choices remain in `layout_selections`. A parent filter such as `order_id = 42` can push beneath expansion and choose a partitioned parent table. An element filter such as `quantity > 1` applies after expansion and does not prune parent partitions using scalar element statistics.

Run `cargo run --example collection_table_plans` for a complete mapping and the actual generated logical plan and SQL. The [runnable example](https://github.com/OrchidDB/OrchidDB/blob/main/examples/collection_table_plans.rs) supplies a complete compiler request.

### Semantics and limits

- Null and empty lists produce zero rows. Null list elements produce a row with null element values. Graph identities with null components are excluded by the existing graph mapping rules.
- Parent columns repeat for each element. Duplicates remain relational duplicates; RDF applies its usual triple-set semantics. Result order is unspecified without ORDER BY.
- Explicit child keys are required for graph identity. Use a stable element key, often combined with the parent key. A parent key alone is not a child identity. Repeated values that duplicate a declared graph key are rejected when whole elements are materialized; they require a distinct child key even though relational projections retain duplicate rows. List positions are not synthesized as identities.
- One collection is expanded per definition. No zipped arrays, implicit Cartesian expansion, nested collection sources, native maps, JSON/text coercion, or outer expansion. Parent columns and exposed struct fields must be scalar. Registered logical views cannot be collection parents; a caller-owned database view may be registered as a physical schema.
- Define physical layout alternatives on the parent tables. Parent-table uniqueness and row counts are not child-table facts. Collection-level supplied constraints are not consumed by the optimizer in this version; parent constraints remain within the input plan.
- Writes through collection-backed node/edge mappings are rejected. Update the containing physical row through its owner instead.
- DataFusion and DuckDB execution are tested. Other dialects depend on existing UNNEST support; no additional portability claim is made. Runtime DuckDB metadata lookups use the parent's unfiltered layout choice; query planning can choose using pushed parent filters.
- Layout byte/file estimates describe the parent scan, not the expanded row count. There is no new estimate for collection length or element selectivity.

## Physical layout alternatives

The merged engine accepts several equivalent physical tables under one logical table name. Node, edge, RDF, and query-backed mappings refer to that logical name. The selector simplifies expressions, pushes predicates through relational projections, and chooses a physical table independently for each scan. Self-joins can select different layouts.

Register physical providers first, then call `GraphMapping::register_logical_source`. The SQL compiler accepts the same descriptors in its optional `logical_sources` array. Existing mappings need no changes. The logical source's schema comes from `default_table`; alternatives must have the same column names, order, types and nullability. Use caller-owned views to normalize different physical schemas.

```json
{
  "name": "events",
  "default_table": "events_by_id",
  "layouts": [
    {
      "table": "events_by_id",
      "specs": [{"spec_id": 0, "fields": []}],
      "partitions": [{"spec_id": 0, "bytes": 2000000, "files": 20}]
    },
    {
      "table": "events_by_region",
      "specs": [{
        "spec_id": 0,
        "fields": [{
          "source_column": "region",
          "source_id": 2,
          "field_id": 1000,
          "transform": {"kind": "identity"}
        }]
      }],
      "partitions": [
        {"spec_id": 0, "bytes": 1000000, "files": 10, "values": {"1000": "east"}},
        {"spec_id": 0, "bytes": 1000000, "files": 10, "values": {"1000": "west"}}
      ]
    }
  ]
}
```

A filter `region = 'east'` selects `events_by_region`. The logical plan contains `TableScan: events_by_region`, and the emitted SQL uses that table. All original predicates remain effective; metadata chooses a table rather than filtering the returned rows itself.

`CompiledSql`, `DagStats`, and `ExecStats` expose `layout_selections`. Each decision includes the logical source, selected physical table, estimated bytes/files/rows, selected partition specs, and candidate estimates/rejection reasons. `logical_plan` shows the resolved plan alongside the existing physical plan and execution measurements. Nested DAG statistics retain their layout decisions. Estimates describe planned scans, not measured Iceberg I/O.

### Partition metadata

Each partition summary has a `spec_id`, `bytes`, `files`, optional `delete_bytes`, optional transformed `values` keyed by partition field ID, and optional inclusive source-column `bounds`:

```json
{"spec_id": 0, "bytes": 1000, "files": 1, "bounds": {"event_id": {"min": "10", "max": "99"}}}
```

Values and bounds are scalar text, parsed using the source or transform output type. JSON null partition values represent nulls. Temporal transform outputs are integer offsets from 1970; bucket outputs are integer bucket IDs. Descriptors retain source IDs and partition field IDs. Each summary uses its own spec, including after partition evolution.

Supported descriptors: `identity`, `bucket` with `buckets`, `truncate` with `width`, `year`, `month`, `day`, `hour`, `void`, and `unknown` with `name`. Bucket hashing follows [Iceberg's hash requirements](https://iceberg.apache.org/spec/#appendix-b-32-bit-hash-requirements). Unsupported type/transform combinations contribute no pruning estimate. Comparisons, literal IN lists, conjunctions and disjunctions participate in estimation; unknown expressions retain their partitions.

Cost is estimated surviving bytes plus delete bytes plus 64 KiB per surviving file. Equal costs retain the default layout. Missing statistics are unknown, not zero; a default with unknown statistics is retained. An empty partition list describes a known empty layout. Statistics must summarize all files in that layout. The engine does not discover Iceberg metadata or scan data to construct these summaries; supply them from the caller's catalog or manifest integration.

Optional `generation` fields on the logical source and layouts exclude layouts whose generation differs. Optional layout `snapshot` strings are included in diagnostics. Omit both for ordinary catalogs; declaring alternatives asserts that they contain equivalent rows, including duplicate multiplicity. Snapshot strings are metadata provenance and do not issue snapshot-pinning SQL.

TOML round trips the descriptors as JSON in `[logical_sources]` / `catalog`, following the existing constraint-catalog convention. Register physical providers after parsing the TOML to bind the sources. Runtime scalar-key lookups use the same selector. Logical sources are read-only in mapped graph persistence; update physical tables and refresh their declarations/statistics separately.

## Equivalent derived and materialized relations

Use `GraphMapping::register_representation_source` to register a collection
expansion, a SQL join or grouped definition, and equivalent materialized tables
under one canonical relation name. Node, edge, RDF, and query-backed mappings can
refer to that name. Predicates guide per-occurrence selection; execution and
compiler statistics expose the selected representation and candidate estimates.
See [equivalent representations](representations.md) for complete descriptors,
examples, persistence, and the equivalence contract. These sources are read-only.
