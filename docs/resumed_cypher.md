# Resumed Cypher read-query work

This checkpoint resumes the interrupted native list-property implementation. It
is a focused correctness change, not completion of work package C1 or a new
full-corpus conformance measurement.

## Completed scope

- Cypher catalog scans provide hidden, native Arrow list columns for encoded
  list properties when the non-null values have a compatible element type.
  Node and relationship scans read current, overlay-aware property values.
  Public property rendering continues to use the existing display column.
- List operands use these columns for UNWIND, quantifiers, indexing, slicing,
  membership, compatible append/prepend operations, and `size()`.
- Encoded properties without a compatible native list column produce an
  explicit unsupported error when used through the list-operand path. In
  particular, heterogeneous list indexing no longer indexes display text.
- SQL scope barriers around dynamic UNWIND preserve the unnested binding name
  and keep nested scalar calls and aggregates outside the UNNEST SELECT.
  Explicit input projections satisfy DataFusion's UNNEST unparser contract.
- Outer UNNEST supplies a typed null element for empty/null lists because SQL
  UNNEST does not honor DataFusion's `preserve_nulls` option. Quantifier results
  retain empty/null inputs and duplicate outer rows.
- Dynamic string slicing supports Unicode characters, inclusive one-based
  bounds, negative bounds, and clamping.

## Validation

Run from the repository root with the default DuckDB feature:

```sh
RUST_MIN_STACK=16777216 cargo test --test cypher_list_properties
RUST_MIN_STACK=16777216 cargo test --test sql_backend_smoke cypher_
```

Results: **5/5 new tests passed**, and **14/14 existing Cypher SQL smoke tests
passed**. New tests cover list size/index/UNWIND/append, nested and string lists,
membership and slicing, Unicode string slicing, heterogeneous-list rejection,
and ANY/ALL/NONE/SINGLE over empty lists, null lists, null elements, and duplicate
outer rows. Existing smoke tests also exercise aggregates and recursive paths.
The larger test-thread stack follows this repository's documented integration
configuration; it is not a production resource guarantee.

The interrupted catalog optimization shares immutable base adjacency and edge
location indexes across clones through `Arc`, using copy-on-write when base
edges are added. Snapshot decoding rebuilds those shared indexes. Its existing
regressions were validated with:

```sh
RUST_MIN_STACK=16777216 cargo test --lib ir::catalog::
```

Result: **22/22 catalog tests passed**, including clone isolation after adding
edges, snapshot roundtrips, grouped-edge endpoint preservation, corruption
rejection, and incremental overlay restoration.

## Remaining work

- No full C1 or read-corpus completion claim follows from these focused tests.
  The broader scope remains in `duckdb_read_query_completion_plan.md`.
- Entirely empty or null encoded list-property scans do not provide enough
  information to infer an element type; no native shadow is created.
- Heterogeneous list values remain unsupported by the uniform Arrow list
  representation.
- Projecting a property into a scalar alias can lose its native shadow
  semantics; this checkpoint does not establish lossless list identity through
  all scope transformations.
- List-comprehension collection ordering, general correlated collection
  semantics, shortest-path completion, and complete read-profile coverage are
  not established by this change.
- No additional path algorithm was implemented. Passing existing recursive
  path smoke tests is regression evidence only.
