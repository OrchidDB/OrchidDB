# Functions and engine dialects

Queries use ordinary names: `bit_count(x)`, `quantile_cont(x, 0.5)`, or
`business_score(x)`. No DuckDB namespace and no generated-grammar changes are
required. Backticks escape function names that are language keywords or contain
operator punctuation.

The design separates three responsibilities, following Calcite's
[operator table](https://calcite.apache.org/javadocAggregate/org/apache/calcite/sql/SqlOperatorTable.html)
and [dialect expression adaptation](https://calcite.apache.org/javadocAggregate/org/apache/calcite/sql/SqlDialect.html):

1. Language lowering preserves existing Cypher, Gremlin, and SPARQL semantics.
   A standard name wins a collision: Cypher `log(x)` still means natural log,
   and lenient casts still return NULL on conversion failure.
2. `ir::functions::OperatorTable` resolves additional expression functions.
   `DuckDbCatalog` reads every overload from the installed
   [`duckdb_functions()` catalog](https://duckdb.org/docs/stable/sql/meta/duckdb_table_functions).
   DuckDB binds argument types, constants, varargs and polymorphic return types.
   Scalar functions, scalar macros and aggregates are expression candidates;
   table functions and pragmas are explicitly rejected in expression position.
3. `ir::rel::sql::functions` transforms SQL expression nodes for the target
   dialect. Rules cover logarithms, precision truncation, sign/NaN, `nanvl`, regex,
   and list distinct/intersection/replacement semantics. Rewrites preserve NULLs,
   evaluation count and list ordering; function-looking text inside strings is
   never replaced. Adding another engine requires its operator-table adapter
   and corresponding SQL dialect rules.

The complete installed catalog is discoverable; this does not mean every SQL
syntax form is available in every graph language. Table sources, SQL window
syntax and DuckDB lambda syntax are not added here. Generic calls are available
through Cypher's existing function-call production and Graph IR calls. SPARQL
and Gremlin retain their language-specific grammar and standard-function lowering.
Engine-specific types or expression forms that cannot cross the Arrow/relational
boundary produce a planning error instead of a guessed return type.

## UDFs and code-based mappings

Register implementations using DuckDB's API on the execution database first.
The `duckdb` dependency enables `vscalar` for Rust scalar UDF registration.
Macros and extension functions can also be discovered from the same database.
Take a catalog snapshot **after** registering them. Rebuild that snapshot when
functions are added, removed, or changed.

For a `MappedGraphEngine` (including mappings over DuckDB/Iceberg views):

```rust,ignore
use std::sync::Arc;
use new_graph::ir::functions::{DuckDbCatalog, FunctionKind, FunctionRegistry};

// Real code UDFs use this same connection:
// engine.executor_mut().connection()?.register_scalar_function::<MyUdf>("score_impl")?;
engine.execute_sql("CREATE MACRO score_impl(x) AS x * 3")?;
let connection = engine.executor_mut().connection()?.try_clone()?;
let catalog = Arc::new(DuckDbCatalog::from_connection(connection)?);
let mut functions = FunctionRegistry::new(catalog);
functions.register_mapping("business_score", "score_impl", FunctionKind::Scalar)?;
functions.register_mapping("median_score", "quantile_cont", FunctionKind::Aggregate)?;
engine.set_operator_table(Arc::new(functions))?;

let result = engine.cypher("RETURN business_score(4)").await?;
```

Mappings can target schema-qualified functions such as `analytics.score_impl`;
SQL rendering quotes each identifier component. A macro can perform argument
reordering or a more involved expression transformation around a code UDF.
Custom code can implement `OperatorTable::prepare_args` to reorder or transform
argument expression trees, alongside `bind` and `target_name` for resolution.
Alias registrations preserve the engine's overload inference. For a macro containing
an aggregate, register its alias with `FunctionKind::Aggregate`; DuckDB catalog
metadata identifies macros without exposing their scalar/aggregate body kind. For implementations
not present in the catalog snapshot, declare their types explicitly:

```rust,ignore
use arrow::datatypes::DataType;
functions.register_typed_mapping(
    "business_score", "score_impl", FunctionKind::Scalar,
    vec![DataType::Int64], DataType::Int64,
)?;
```

Repeat typed registrations to add overloads. Declarations require exact Arrow
argument types (or NULL), reject ambiguous overloads, and emit explicit SQL casts
to preserve the declared overload for literals and NULLs. They do not install the UDF
implementation. Registrations cannot override language-defined names. Scalar and
aggregate aliases share the same registry, so aggregate aliases participate in
`DISTINCT`, grouping, nested-aggregate validation and empty-input handling.

For lower-level callers, use `with_operator_table(Arc<dyn OperatorTable>, || ...)`
around **both synchronous frontend planning and relational lowering**, then
prepare/execute the resulting plan through `DuckDbExecutor::from_connection`.
Do not wrap an async future in that closure: selection is scoped to synchronous
work on the current thread. Nested scopes and panics restore the previous table;
registrations never leak to another thread. `MappedGraphEngine` manages these
scopes internally across its asynchronous read, explain and update APIs. The
managed in-memory `GraphEngine` hybrid interpreter API does not install this
session-scoped UDF configuration.

Binding does not evaluate function implementations. Return-type inference uses
DuckDB's binder and a NULL-only Arrow schema query; `cast_to_type` discards its
type-reference expression during binding. UDF calls remain volatile planning
objects so DataFusion/interpreter folding cannot execute them. Execution belongs
to DuckDB; attempting these calls through DataFusion or another SQL dialect fails
explicitly.

## Ownership and verification

- `src/ir/functions`: operator catalog, typed call carriers, mappings and scope.
- `src/ir/rel/sql/functions.rs`: dialect expression transformations.
- `src/language/cypher/planner/lowering/project/aggregate.rs`: aggregate classification.
- `src/mapped_engine.rs`: execution-session configuration.
- `tests/engine_functions.rs`: scalar binding, NULL behavior and name precedence.
- `tests/duckdb_native_aggregates.rs`: aggregates and exhaustive catalog-name parsing.
- `tests/duckdb_udf_mappings.rs`: real Rust UDFs, macros, declarations, isolation,
  and mapped read/update/explain integration.

The SQL dialect tests compare transformed expressions against actual DataFusion
results and bundled DuckDB, including NULLs, NaN, empty lists and list ordering.
The UDF tests assert that code is not invoked during planning or SQL preparation.

Validation for this change: 486 tests passed across the non-corpus suite and
final focused reruns, with four existing ignored tests. The full Ladybug and
TinkerPop corpus sweeps were not rerun. `cargo check --no-default-features --lib`
also passed. Generated grammar files are unchanged.
