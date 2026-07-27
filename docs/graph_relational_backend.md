# Graph IR Relational Backend

This is the placement guide for future Cypher, Gremlin, and SQL execution work.

## Layers

- `src/ir/plan.rs` is the durable graph algebra. Language planners should keep producing Graph IR, including operators that are not relationally executable yet.
- `src/ir/df.rs` is the DataFusion extension-plan bridge. It preserves Graph IR operators as DataFusion `Extension` nodes so HEP and rule passes can inspect, rewrite, and round-trip graph plans.
- `src/ir/rel/` is the executable relational backend. It decomposes Graph IR into base relational DataFusion `LogicalPlan`s and runs those plans with `SessionContext::execute_logical_plan`.
- SQL generation belongs after relational decomposition. SQL dialect rules should consume DataFusion logical plans or SQL islands, not Graph IR directly.

## SQL Islands

The backend should lower only regions with relational semantics that are known to be equivalent. Unsupported graph regions must remain classified with a reason; they should not be silently approximated.

An island boundary is a semantic boundary, not just a syntax boundary. Good island candidates include label/type scans, property filters, one-hop expands, projections, sort/slice, row distinct, joins, unions, and simple aggregates. Poor candidates include path materialization, graph mutations, side effects, provider-specific procedures, and graph algorithms unless a rule proves an equivalent relational form.

Future dialect support should be rule driven:

- capability rules decide whether a DataFusion plan region can be emitted for a dialect;
- rewrite rules normalize DataFusion plan shapes into dialect-friendly forms;
- unparsing rules turn an accepted island into SQL;
- non-SQL regions keep their Graph IR identity and execute through a graph-aware path.

## Recursion

Variable-length expansion and repeat-style traversal should target DataFusion recursive queries when the semantics can be expressed as a recursive relation. The recursive lowering should live in `src/ir/rel/recursive.rs`, build `LogicalPlan::RecursiveQuery` through DataFusion APIs, and keep path/history columns explicit when uniqueness or path output is part of the contract.

Do not add a lower execution layer below DataFusion for this. The next executable representation after Graph IR decomposition is the DataFusion logical plan.

## Ownership

- Language-specific planning changes stay under `src/language/<language>/planner`.
- Shared semantic analysis and normalization stay before backend lowering.
- DataFusion relational lowering stays under `src/ir/rel`.
- DataFusion extension bridge and HEP integration stay under `src/ir/df.rs`.
- Conformance progress harnesses stay under `tests/` and should reuse existing Cypher and Gremlin case infrastructure.

## Bring Your Own Schema (BYOS) Mapping

`src/ir/rel/mapping.rs` lets graph labels and edge types resolve against a
user-owned relational schema instead of tables generated from a
`PropertyGraph`. A `GraphMapping` describes, per node label and edge type,
the backing table **or SQL query**, the id column, and the graph-property →
source-column mapping. Install it on `RelBackendOptions::mapping` and every
node/relationship scan lowers through the mapping; the property-graph catalog
is not consulted (pass an empty `PropertyGraph` to `RelBackend::lower`).

### Mapping config

Programmatic:

```rust
use new_graph::ir::rel::mapping::{EdgeMapping, GraphMapping, NodeMapping};

let mut mapping = GraphMapping::new();
// Providers back the physical table names (schema; and data for in-process).
mapping.register_table("customers", Arc::new(mem_table));
mapping.register_table("orders", Arc::new(orders_table));
// SQL-defined view over registered tables (a real DataFusion ViewTable).
mapping.register_view(
    "high_value_customers",
    "SELECT c.cust_id, c.full_name FROM customers c \
     JOIN orders o ON o.cust_id = c.cust_id WHERE o.total > 400.0",
)?;

mapping.map_node(
    NodeMapping::table("Person", "customers", "cust_id")
        .property("name", "full_name")
        .property("age", "age"),
);
mapping.map_node(
    NodeMapping::query("Vip",
        "SELECT cust_id, full_name FROM customers WHERE age >= 30", "cust_id")
        .property("name", "full_name"),
);
mapping.map_edge(
    EdgeMapping::table("ORDERED", "orders", "cust_id", "order_id", "Person", "Order")
        .with_id("order_id")
        .property("total", "total"),
);

let backend = RelBackend::with_options(RelBackendOptions {
    mapping: Some(Arc::new(mapping)),
    ..RelBackendOptions::default()
});
```

Serialized (a hand-rolled TOML subset, `GraphMapping::from_toml`/`to_toml`;
providers are registered separately after parsing):

```toml
[node.Person]
table = "customers"          # or: query = "SELECT ..."
id = "cust_id"

[node.Person.properties]
name = "full_name"
age = "age"

[edge.ORDERED]
table = "orders"
src = "cust_id"
dst = "order_id"
src_label = "Person"
dst_label = "Order"
edge_id = "order_id"         # optional; defaults to src

[edge.ORDERED.properties]
total = "total"
```

### How scans lower

- **Table-backed** mappings become a `TableScan` of the user table under a
  projection that renames/casts source columns into the binding-prefixed
  shape the rest of the lowering expects (`p__id`, `p__label`,
  `p__prop__name`, ...). Labels are projected as literals; id columns are
  cast to `BIGINT` (they must be integer-typed and unique per label).
- **Query-backed** mappings are parsed with datafusion-sql
  (`SqlToRel` over the mapping's registered tables) and spliced in as a
  subplan, so DataFusion's optimizer pushes filters and projections straight
  through the defining query. `register_view` names such a plan so
  table-backed mappings can point at it like any table.
- Scans that cover several labels (e.g. an unlabeled `MATCH (n)`) union the
  per-label plans; properties a label does not map project as typed NULLs.

### Execution paths

- **In-process (DataFusion)**: register real providers (`MemTable`, Arrow) on
  the mapping and run `execute_lowered` as usual.
- **External SQL (DuckDB/Postgres)**: the mapped table/view names are assumed
  to exist in the target database. Use
  `sql::prepare_with_external(&lowered, dialect, &mapping.physical_table_names())`
  — the user's tables are *not* re-materialized as `CREATE TABLE`/`INSERT`
  setup, and query/view-backed sources appear in the generated SQL as inlined
  derived tables. `register_table_schema` registers a schema-only provider
  when the data lives solely in the external database.

End-to-end example with both engines, a DuckDB `VIEW`, Cypher and Gremlin
traversals, and optimizer-pushdown assertions: `tests/byos_smoke.rs`.

### Current limitations

- Id columns must be integer-typed; parallel edges need `with_id`/`edge_id`
  to stay distinguishable (the edge id defaults to the source column).
- One mapping per backend: when set, it replaces the `PropertyGraph` catalog
  for all scans in the plan.
- Mapped property names with mixed source types across labels are rejected.
