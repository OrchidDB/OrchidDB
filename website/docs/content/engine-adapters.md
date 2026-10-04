# Engine adapters

An engine adapter translates OrchidDB's typed relational plan into the engine's
SQL. It can rewrite expressions, transform entire relations, and encode values
crossing an execution boundary. Implementations are Rust code; configuration
selects a registered implementation and supplies source facts.

Search uses this same extension point. There is no separate search-backend
registry. The [Search guide](search.md) shows pgvector and duckdb-lance source
metadata and relationship declarations.

## Register an engine

Implement `orchiddb::ir::rel::sql::DialectAdapter`, also exported as
`EngineAdapter`. Required methods are `name`, `parser_dialect`,
`unparser_dialect`, and `sql_type`. Supply `exchange_literal` for typed inputs
crossing islands or binding dependent operations. Methods return explicit
errors for unsupported types and operations.

Register a static implementation before compiling queries:

```rust
use orchiddb::ir::rel::sql::SqlDialect;

static WAREHOUSE: Warehouse = Warehouse;
let dialect = SqlDialect::register(&WAREHOUSE)?;
```

`Warehouse` is your adapter type. Its `name()` becomes the compiler's `dialect`
value, including engine descriptors in a federated request. Runtime connections
remain caller-owned and implement the existing session interface. Registering
the same adapter instance twice is allowed; replacing a registered name is not.

The [complete tested adapter](https://github.com/OrchidDB/OrchidDB/blob/main/tests/engine_adapter.rs)
implements a third dialect, scalar mappings, AST rewriting, a table-function
transformation, and typed input binding without modifying core dispatch.

## Transform expressions and relations

Use `function_mapping` to provide a logical function's SQL expression and
ordering mapping. Use `rewrite_expression` and `rewrite_query` for SQL AST
transformations that need code. Custom dialects do not inherit DuckDB function
implementations: unsupported functions must be rejected or explicitly mapped.

For a row-producing operation, implement `lower_relation`. This example method
renames a logical table function and delegates SQL construction to the shared
relational lowering:

```rust
fn lower_relation(
    &self,
    plan: &datafusion::logical_expr::LogicalPlan,
    context: &orchiddb::ir::rel::sql::lowering::LoweringContext,
) -> orchiddb::ir::rel::sql::SqlResult<
    Option<orchiddb::ir::rel::sql::lowering::RelationLowering>,
> {
    use datafusion::logical_expr::LogicalPlan;
    use orchiddb::ir::rel::{
        dependent::TableFunction,
        sql::{SqlError, lowering::builtin_lower_relation},
    };
    if let LogicalPlan::Extension(extension) = plan {
        if let Some(function) = extension.node.as_any().downcast_ref::<TableFunction>() {
            if function.name != ["expand_numbers"] {
                return Err(SqlError::Unsupported("warehouse table function".into()));
            }
            let mut physical = function.clone();
            physical.name = vec!["warehouse_range".into()];
            return builtin_lower_relation(&physical.into_plan(), context);
        }
    }
    Ok(None)
}
```

The return value can be any of:

| Result | Meaning |
| --- | --- |
| `RelationLowering::Sql(statement)` | A SQL AST for the relation, composable inside an island. |
| `RelationLowering::Rewrite(plan)` | An equivalent logical plan for further lowering. |
| `RelationLowering::Dependent { source, template, schema }` | Execute after binding each source row into the owning engine's SQL template. |
| `None` | This hook does not implement the operation. |

Rewrites must preserve the output schema. Dependent templates must preserve the
output schema, use the owning dialect, and bind one parameter per source column.
For custom extension nodes with multiple inputs, `relation_placement` identifies
which source and target inputs determine ownership. Ordinary relational plans
continue through the existing SQL-island planner.

## Table functions and binding

`TableFunction::new` takes a multipart name, typed `Expr` arguments, an optional
source plan, an `ArgumentBinding`, and the function's output `DFSchemaRef`.
Its resulting schema contains source columns followed by function output columns.
Arguments are checked against the source schema. Source and output column names
must be distinct; project unique aliases before constructing the function when
joined inputs contain duplicate names.

- `ArgumentBinding::Correlated` allows the engine to consume source columns
  inside the same statement.
- `ArgumentBinding::PrepareTime` requires values before the engine prepares the
  statement, so lowering creates a dependent operation.

`LoweringContext.mode` can also request a bound operation when ownership crosses
engines. `SqlTemplate::bind` uses typed values and the registered dialect's
literal codec. Nulls, empty collections, numeric types, and nested values need
explicit encodings; unsupported representations fail.

Predicates remain above a table-function boundary until an adapter explicitly
implements a valid pushdown. Moving a filter after a top-k operation changes its
meaning; accepting a pushdown means preserving its placement and semantics.

## Search and source metadata

`GraphMapping::register_source_metadata` and the compiler's `source_metadata`
array describe each table's physical format, source options, and indexes:

```toml
[[source_metadata]]
table = "documents"
format = "vendor_collection"
options = { collection = "articles" }
indexes = [{ column = "embedding", metric = "cosine", options = { access_method = "vendor_ann" } }]
```

Custom format and option names are retained for the owning adapter to validate.
Use `GraphMapping::try_to_toml()` when serializing arbitrary source options. JSON
null options remain valid metadata but cannot be represented in TOML; this
method returns an error instead of dropping them. The legacy `to_toml()` method
retains its infallible signature and panics on unrepresentable metadata.
A search transformation inspects `RankedJoin`: it carries query/document
expressions, eligibility, per-source keys, score ordering, limit, retrieval
requirements, and the selected source/index metadata. Return the engine's
index-usable SQL AST or a dependent operation. Reject unsupported metrics,
filters, or retrieval requirements explicitly. PostgreSQL's rule uses distance
ordering; DuckDB's Lance rule emits bound search calls.

The same interfaces support future JSON work: typed scalar expressions can use
AST rewrites; row expansion can use relational nodes; engine-specific nested
value representations use codecs. Full JSON lowering is a separate feature and
is not implemented by this adapter change.

## Execute dependent operations

Compiled transfers expose an optional `operation` descriptor with its owner
engine, input columns, and SQL template. `federation::execute` handles these
through caller-owned sessions. JSON clients execute source SQL, call
`op: "bind_operation"` with the source rows, execute returned statements on the
specified engine, and feed output rows into the ordinary `bind` operation.

The former `search` descriptor, `bind_search` command, and `search_indexes`
configuration are accepted as compatibility inputs. New plans use the general
operation names. Execution errors propagate without retrying a different access
method.
