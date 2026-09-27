# Supplied relational constraints

Constraints are an engine-neutral input to OrchidDB. `GraphMapping::set_constraints`
accepts a `ConstraintCatalog`; the SQL compiler accepts the same structure in its
optional `constraints` field. Neither entry point discovers constraints from a
database. `constraints::duckdb` is an optional extraction and validation adapter,
compiled only with the `duckdb` feature. Other databases can supply the same catalog.

## Contract and lifecycle

The catalog maps the exact registered source name to named facts:

- `unique`: ordered column list, with `nulls_equal` distinguishing ordinary SQL
  nullable uniqueness from uniqueness that includes nulls.
- `non_null`: columns guaranteed not to contain nulls. A primary key is represented
  as a unique fact and a non-null fact.
- `foreign_key`: ordered child columns, target source name, and referenced columns.
  This is MATCH SIMPLE inclusion: rows with any null child key need not match.
- `functional_dependency`: determinant and dependent column lists. Values use
  NULL-equal equality. A dependency alone does not assert row uniqueness.

Composite relational keys are supported. They do not introduce synthetic graph
IDs or change the existing single-scalar identity mapping contract. Facts must
hold under the equality semantics of the executing engine and the mapped column
types; a database adapter must not silently translate a different collation or
comparison domain into this contract.

Each fact carries evidence:

| Evidence | Can justify a rewrite? |
| --- | --- |
| `enforced` | Yes. The supplying engine/caller guarantees it for every execution using this catalog. |
| `validated` with a scope | Only when `set_constraint_scope` / compiler `constraint_scope` explicitly matches that immutable snapshot. |
| `declared` (default) | No. Merely documenting an obligation is insufficient. |
| `estimate` | No. Statistics cannot justify changing results or removing errors. |

`revision` identifies a caller-owned catalog version. Replacing the catalog builds
new source bindings; SQL-defined views are rebound against the current catalog.
Cyclic view replacements are rejected. There is no shared optimized-plan cache.
An embedding application caching compiled SQL must include the catalog revision,
schemas, mapping definitions, and validation scope in its cache identity and
invalidate that cache after a relevant schema/constraint change.

A validated scope is an explicit caller contract, not a transaction ID detected
by OrchidDB. Keep the corresponding snapshot immutable through compilation and
execution. Changing the snapshot or writing invalidates that evidence. Mapped
`GraphEngine` statements clear caller snapshot activation because they may write
or open a different transaction; they consume persistent enforcement contracts.
Thus a one-time validation never becomes a permanent engine optimization promise.
No constraint is inferred from `id`, a mapped label, or an endpoint declaration.

## Rust and JSON

```rust,ignore
use orchiddb::ir::rel::constraints::{ConstraintCatalog, Constraint, Fact};

let mut constraints = ConstraintCatalog::default();
constraints.insert("people", Constraint::enforced("people_key", Fact::Unique {
    columns: vec!["id".into()], nulls_equal: false,
}));
constraints.insert("people", Constraint::enforced("people_id_required", Fact::NonNull {
    columns: vec!["id".into()],
}));
mapping.set_constraints(constraints);
mapping.validate_constraints()?; // names, columns, FK arity and types; no data scan
```

The equivalent compiler request field is:

```json
{
  "constraints": {
    "revision": "schema-42",
    "tables": {
      "people": [
        {"name":"people_key", "evidence":{"kind":"enforced"},
         "fact":{"kind":"unique","columns":["id"],"nulls_equal":false}},
        {"name":"people_id_required", "evidence":{"kind":"enforced"},
         "fact":{"kind":"non_null","columns":["id"]}}
      ]
    }
  }
}
```

The catalog supports Serde JSON round trips. Mapping TOML carries it as a JSON
string in `[constraints] catalog = "..."`; snapshot activation is deliberately
not serialized. Compiler responses add `constraint_proofs` to the existing
version, dialect, SQL and fields.

## DuckDB helpers

```rust,ignore
use orchiddb::ir::rel::constraints::duckdb::{extract, TableBinding};

let catalog = extract(&connection, &[
    TableBinding::new("people", "my_database", "main", "people"),
    TableBinding::new("orders", "my_database", "main", "orders"),
])?;
mapping.set_constraints(catalog);
```

Bindings explicitly name the catalog, schema and table, independently of the
name used in the mapping. Extraction reads DuckDB catalog metadata, preserving
composite column order. It extracts primary keys, unique constraints, non-null
constraints and foreign keys between supplied bindings; it does not sample data
or infer keys from unique-looking values. The revision hashes extracted metadata.
Check constraints and expression indexes are not interpreted as key proofs.
The helper rejects non-binary default collations and explicitly collated table
definitions rather than asserting keys under an incompatible comparison domain.

`duckdb::validate(connection, bindings, catalog, scope)` checks supplied facts in
the caller's transaction and returns snapshot-scoped evidence. Uniqueness,
non-nullability, inclusion, and dependencies are checked by SQL; source rows are
not copied into OrchidDB. Dependency validation can require a source self-join.
Validation is explicit and is not performed automatically during query planning.

## Propagation and rewrites

`constraints::analyze(plan)` returns unique/equality keys, non-null columns,
functional dependencies, source-column lineage, foreign keys, source completeness
and evidence. Projection and renaming preserve surviving facts; repeated aliases
of a column determine each other. Identity casts and total integer widening
preserve keys. Other casts and arbitrary expressions do not carry key lineage.
Filters preserve uniqueness but remove source completeness; simple predicates
can prove non-nullability. Joins account for fan-out and null extension.
Aggregations establish grouping keys. Limits retain uniqueness but lose coverage.
Union, recursive and opaque native operators are proof barriers unless a specific
rule establishes the necessary facts. The same logic is used before SQL-island
placement and by standalone compilation.

`constraints::optimize(plan)` returns the plan and a structured `RewriteProof`
list. Execution reports expose proofs in `ExecStats.constraint_proofs`, including
executed nested plans. Each proof names the rule, its condition and source evidence.
The original execution-plan review is unchanged.

Implemented rules:

- Remove full-row DISTINCT when a NULL-equal key determines row uniqueness.
- Reduce scalar grouping columns determined by remaining grouping columns,
  reconstructing the original output columns. Retain at least one grouping key
  to preserve empty-input behavior. The initial representative implementation
  covers Int32, Int64 and UTF-8 fields.
- Remove an unused right side of a left join when it matches at most once.
- Remove an unused right side of an inner join when a non-null FK or identical
  source identity proves exactly one match.
- Remove a self-join and substitute right payload columns from the same source row.
- Remove a redundant semi-join when every left row has a proven target match,
  preserving every left occurrence.
- Remove a contained branch of a two-input set union, retaining its DISTINCT
  and original output names. Containment covers pure selection/projection/alias
  lenses over one source with identical output lineage and inclusion of normalized
  predicate sets. It does not perform general arithmetic predicate implication.

These rules retain filtered targets when existence is unproven, parallel edges,
nullable-key duplicates, and UNION ALL occurrences. They do not discard subplans
containing potentially throwing/volatile expressions. An endpoint FK establishes
node existence, not one-to-one traversal. `relationship_multiplicity` reports
endpoint existence and degree upper bounds separately; it does not invent a
minimum number of incident edges.

DataFusion 53's generic DISTINCT rewrite can confuse nullable or fan-out
functional dependencies with row uniqueness. Supplied facts therefore remain in
OrchidDB's proof model instead of being published as weaker generic DataFusion
constraints. The shared session also guards DISTINCT conversion for constrained
plans. Statistics never enter this proof path.

## Verification

`tests/relational_constraints.rs` executes before/after queries for DISTINCT,
FK joins, self-joins, grouping reduction, semi-join containment and set-union
containment. It also covers composite and nullable keys, duplicate occurrences,
filtered targets, throwing projections, scope expiry, catalog/view invalidation,
serialization, DuckDB extraction/validation, PostgreSQL SQL compilation, and an
actual mapped Cypher query whose lens self-join disappears from the executed SQL.
Final conformance preserved all 6,382 baseline passes: 3,897 openCypher, 1,511
TinkerPop and 974 RDF. The 74 not-applicable and 77 skipped RDF cases are unchanged,
as are every case definition and outcome. Local checks passed 360 Rust tests,
64 JVM tests and four Python cost-report tests; default and DuckDB all-targets
compilation passed. See the [validation record](relational-constraints-validation.json).

Every conformance case retains [per-query costs](query-cost.md) for ranking lowering
opportunities and comparing subsequent runs. Complete boundary-work coverage is
available for 5,116 cases after correcting Gremlin attribution to exclude
assertion-helper requests; elapsed-only and unexecuted cases are explicitly
marked. This correction reuses recorded measurements and does not represent a
new suite run. The existing execution-plan review and examples remain unchanged.
