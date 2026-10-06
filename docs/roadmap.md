# Extension direction

The DuckDB migration is implemented: shared language compilation, host relational
execution, existing graph kernels, managed storage, transactions, native Cypher and
Gremlin syntax, and real Iceberg/Lance sources. The pinned Cypher and Gremlin suites
pass completely; existing SPARQL scope is preserved. See [verification](verification.md).

Further work is separate from this migration:

- Validate and package the other supported Linux/macOS build targets.
- Establish signed extension distribution and version-specific release artifacts.
- Expose the existing computed relationship/search planner through graph DDL.
- Consider unquoted graph subqueries inside SQL in addition to table functions.

These items are not claims about the current release. Keep future work focused on
adapting existing compiler and storage capabilities rather than adding duplicate
language evaluators or execution engines.
