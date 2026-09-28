# Equivalent representations

Map one logical relation to several equivalent ways of reading its rows. An
`Item` mapping can read `orders.items` through collection expansion or a flat
`order_items` table. A relationship can use a join definition or its prejoined
table; a summary can use a grouped SQL definition or a materialized summary.
Cypher, Gremlin, and SPARQL keep the same logical vocabulary while OrchidDB
chooses an input for each occurrence in the relational plan.

## Declare the alternatives

Register the physical schemas and collection definitions first. Then call
`GraphMapping::register_representation_source`, or provide
`representation_sources` in a compiler request. This fragment assumes
`expanded_items` is a collection relation with `order_id`, `item_id`, `sku`, and
`quantity`, and `flat` has `oid`, `line`, `product`, and `qty`:

```json
{
  "representation_sources": [{
    "name": "items",
    "generation": "load-42",
    "default_representation": "nested",
    "representations": [
      {
        "name": "nested",
        "generation": "load-42",
        "source": {"kind": "table", "name": "expanded_items"},
        "average_list_length": 2,
        "statistics": [{
          "table": "orders",
          "specs": [{"spec_id": 0, "fields": []}],
          "partitions": [
            {"spec_id": 0, "bytes": 1000, "files": 1, "rows": 10,
             "bounds": {"id": {"min": "1", "max": "49"}}},
            {"spec_id": 0, "bytes": 99000, "files": 1, "rows": 10,
             "bounds": {"id": {"min": "50", "max": "99"}}}
          ]
        }]
      },
      {
        "name": "flat_sku",
        "generation": "load-42",
        "source": {"kind": "table", "name": "flat"},
        "columns": {"order_id": "oid", "item_id": "line",
                    "sku": "product", "quantity": "qty"},
        "statistics": [{
          "table": "flat",
          "specs": [{"spec_id": 0, "fields": []}],
          "partitions": [
            {"spec_id": 0, "bytes": 10000, "files": 1, "rows": 10,
             "bounds": {"product": {"min": "a", "max": "m"}}},
            {"spec_id": 0, "bytes": 10000, "files": 1, "rows": 10,
             "bounds": {"product": {"min": "n", "max": "z"}}}
          ]
        }]
      }
    ]
  }],
  "nodes": [{
    "label": "Item", "table": "items", "id": ["order_id", "item_id"],
    "properties": {"order_id": "order_id", "sku": "sku", "quantity": "quantity"}
  }]
}
```

`columns` maps canonical output names to source columns. An empty map exposes
all source columns. Alternatives must expose the same names and Arrow types;
the default defines their order, and nullable output fields accommodate any
nullable alternative. Explicit parent/child key columns remain the graph
identity in every alternative.

`source.kind = "table"` accepts physical tables, layout sources, collection
sources, and other representation sources. `source.kind = "query"` takes a
single read-only SQL query planned against the same catalog. For example:

```json
{"name": "definition", "source": {"kind": "query", "sql":
 "SELECT o.customer_id, f.product AS sku FROM orders o JOIN flat f ON o.id = f.oid"}}
```

Or use `SELECT oid AS order_id, sum(qty) AS total FROM flat GROUP BY oid` as a
summary definition. Register its equivalent materialized table as another
candidate with the same output schema. Grouping, filters, duplicate behavior,
and any casts belong in the definition; OrchidDB does not infer them from the
materialized table's name.

Compiler requests bind derived definitions in dependency order, allowing forward
references. Rust registration requires dependencies already registered. TOML
stores definitions as a JSON array string in `[representation_sources]` /
`catalog`. After loading TOML, register physical providers to bind the dependent
sources. Replacing a provider rebuilds dependent definitions.

## Selection and plan statistics

Filters are pushed into each alternative before comparing its scans. Parent-key
filters can prune the parent scan below an expansion; element filters remain
after expansion. Column aliases translate predicates into physical column names.
With a generated snapshot, connected search also compares whole-plan costs after
join ordering and legal neighbor-key restrictions. A selective neighbor can add a
parent semijoin before collection expansion; the original binding join remains.
`optimizer_decisions` reports the connected choice and its total estimated work.
Each self-join occurrence can choose a different representation. Residual
predicates remain in the plan; selection does not move a limit ahead of them.

For the example metadata:

| Query predicate | Chosen representation | Estimated bytes | Estimated files | Estimated expanded rows |
| --- | --- | ---: | ---: | ---: |
| `i.order_id = 1` | `nested` | 1,000 | 1 | 20 |
| `i.sku = 'a'` | `flat_sku` | 10,000 | 1 | — |
| No predicate | `flat_sku` | 20,000 | 2 | — |

For example, both queries use the same `Item` mapping:

```cypher
MATCH (i:Item) WHERE i.order_id = 1 RETURN i.sku
MATCH (i:Item) WHERE i.sku = 'a' RETURN i.order_id
```

Their selected logical structures are shown below, with identity projections,
aliases, and repeated residual filters omitted for readability:

```text
order_id = 1                       sku = 'a'
Project sku                        Project order_id
  Expand orders.items                Canonical columns: oid -> order_id, product -> sku
    Filter orders.id = 1               Filter flat.product = 'a'
      Scan orders                        Scan flat
```

The first uses `nested` (1,000 estimated bytes); the second uses `flat_sku`
(10,000 estimated bytes). The estimates depend on the supplied metadata above.
One query can use both:

```cypher
MATCH (a:Item), (b:Item)
WHERE a.order_id = 1 AND b.sku = 'a'
RETURN a.sku, b.order_id LIMIT 2
```

Its `representation_selections` contains `nested` for `a` and `flat_sku` for `b`.
The runnable example below emits the complete plans and statistics.

`statistics` uses the existing [partition metadata format](mapping-reference.md#physical-layout-alternatives),
with optional `rows` per partition. Supply metadata for every physical scan of a
candidate, including each input of a join. If an input is a logical layout source,
put its statistics on that source: representation selection composes with layout
selection instead of duplicating metadata.

Without a generated snapshot, cost uses supplied surviving bytes (including delete
bytes), plus 65,536 per file, plus eight per estimated expanded row. In that mode,
expanded rows need one list expansion without a join, supplied input row counts
and `average_list_length`.

With [generated statistics](statistics.md), the shared estimator supplies source,
filter, join, grouping and expansion costs. Collected list lengths can replace a
manual average; generated source costs allow candidates without manifest metrics
to compete. Supplied manifest pruning and file costs still contribute where
available. Unknown components stay visible. These are planning heuristics, not
measured execution time.
Unknown scan costs retain the declared default if its cost is unknown; otherwise
only candidates with known costs compete. The default wins cost ties.

`CompiledSql.representation_selections` and `QueryResult.stats.representation_selections`
report the logical source, chosen representation and definition, generation,
pushed predicates, estimates, underlying scan decisions, and all candidate
reasons. `logical_plan` shows the actual selected scan, join, aggregate, or
`Unnest`; `layout_selections` contains the chosen physical scans. Estimates are
metadata estimates, not measured I/O. For RDF, use `sparql_dataset` to retain
this statistics wrapper.

Run the complete mapping and plan example from the Rust core directory:

```sh
cargo run --example representation_plans
```

The example emits JSON containing the complete compiler request and three plans,
including a self-join that chooses nested and flat inputs independently. Its
catalog is in `examples/data/representation_sources.json`. Regression tests also
execute join and aggregate alternatives and compare their results with their
materializations.

## Equivalence and limits

Registration asserts equality of complete row sets, values, nulls, and duplicate
multiplicities. Matching column types cannot establish that equality. Collection
expansion drops empty/null lists but preserves duplicate and null elements; its
materialization must do the same. Keys must identify the same logical entities
and relationships regardless of the chosen representation.

A candidate is eligible only when its `generation` equals the source's generation;
the default must be current. Empty generations remain valid for unversioned
catalogs. Generations are caller assertions, not a refresh service or an Iceberg
snapshot pin. Update data, generation declarations, and statistics together.

All representation sources are read-only, including mapped graph and RDF writes.
Maintain their physical inputs separately. The feature selects among explicitly
registered equivalent relations; it does not match arbitrary query subplans to
unregistered views, compensate partial materializations, roll up different
aggregate groupings, create/refresh tables, or import Iceberg manifests. Existing
SQL dialect and collection-type restrictions still apply. Include definitions,
generations, schemas, statistics, and parameter values in plan cache keys.

## Optional generated statistics

[Generate statistics once](statistics.md) to collect bounded source summaries and
automatically use the cached snapshot for source selection, cardinality estimates
and supported filter ordering in Cypher, Gremlin and SPARQL. The guide covers
client/engine ownership, plan diagnostics and partial coverage.
