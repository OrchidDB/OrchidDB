# DuckDB extension architecture

Orchid is a DuckDB extension. Cypher and Gremlin have native parser entry points;
SPARQL uses the existing versioned mapping request. All reuse the same language
frontends, graph IR, relational lowering, scalar kernels, and persistence codecs.

```text
Cypher / Gremlin / SPARQL frontends
                |
        Existing graph IR
                |
   Relational and kernel compilation
        /                   \
DuckDB relational plans    Existing graph kernels
        \                   /
       DuckDB physical execution
                |
      Host connection / transaction
                |
 DuckDB tables / Iceberg / Lance / views
```

`extension/src/orchid_extension.cpp` integrates the DuckDB parser, binder, physical
operators, and Arrow transport. `extension/compiler` exposes the existing Rust
compiler and kernel descriptors through a C ABI. SQL regions are bound by DuckDB;
residual graph operators call the shared runtime kernels. DataFusion is a planning
dependency, not an extension execution engine. No PostgreSQL driver or second
DuckDB connection is linked into the extension.

`src/ir/rel/runtime/program.rs` holds shared compiled program definitions.
`src/ir/rel/runtime/host.rs` supplies the host execution boundary; the retained
`legacy.rs` adapter serves existing internal library tests. The extension does not
fall back to that adapter. `src/ir/rel/host` contains extracted mapped source,
managed storage, and persistence access through the host transaction.

Each execution owns its graph overlay, correlation state, side effects, and
cancellation token. Nested kernels run already compiled subplans on that same
transaction. Multi-input stateful branches execute in declared order. Immutable
prepared plans are cached within a query; borrowed state is detached after each
invocation and released at query end. No effects happen during binding or EXPLAIN.

`src/ir/rel/native_values` and catalog transport reuse native value and identity
codecs for paths, graph elements, collections, properties, and RDF terms. Host
values do not pass through a second language evaluator.

Mapped graph DDL discovers schemas through DuckDB and persists definitions as
views. Managed graph records use the existing storage codecs. Iceberg and Lance
remain ordinary DuckDB sources, with their own scan and index execution.

See [extension guide](../extension/README.md), [runtime](runtime.md), and
[verification](verification.md). Retained legacy library adapters and compatibility
references support reuse and regression testing; they are not public client products.
