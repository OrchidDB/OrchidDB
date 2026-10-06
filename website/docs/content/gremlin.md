# Gremlin

```sql
GREMLIN social g.V().hasLabel('Person').out('FOLLOWS').values('name');
EXPLAIN GREMLIN social g.V().has('age', gt(35)).values('name');
```

Use the [quickstart graph](quickstart.md). `orchid_gremlin(graph, query)` is the
corresponding SQL table function. Typed bindings are available through the advanced
mapping protocol. Traversals reuse the existing Gremlin compiler and kernels.

Managed graphs support mutations, multi-properties, and meta-properties. Existing
JVM callback and graph-computer kernels use the repository's JVM modules and
classpath; they are not replaced by a second traversal evaluator.
See [conformance](conformance.md#gremlin) for the original provider assertion coverage.
