Traverse nodes and relationships step by step with Gremlin, then project the values your application needs.

## Start a traversal

Crabgraph accepts Gremlin traversal text through `gremlin(query).await`. In the CLI, select it with `--language gremlin`.

For JVM callbacks and TinkerPop vertex programs, use the [native-backed JVM executor](https://github.com/henneberger/new-graph/blob/main/docs/gremlin-jvm-executor.md) and [GraphComputer API](https://github.com/henneberger/new-graph/blob/main/docs/gremlin-graphcomputer.md). The [compatibility matrix](/conformance.html#language-tinkerpop) reports native Rust, JVM OLTP and GraphComputer execution separately.

```gremlin
g.V().hasLabel('Person').values('name')
```

`g.V()` starts at vertices, `hasLabel` selects a label, and `values` projects a property. Examples here use the [mapped tutorial](/mapped-graphs.html).

## Filter vertices

```gremlin
g.V().hasLabel('Person').has('name', 'alice').values('age')
```

The tutorial returns `30`. Predicates express comparisons:

```gremlin
g.V().hasLabel('Person').has('age', gt(29)).values('name').order()
```

This selects alice and carol.

## Follow relationships

```gremlin
g.V().hasLabel('Person').has('name', 'alice')
  .out('FOLLOWS').values('name').order()
```

Use `out(type)` to follow relationships from source to destination and `in(type)` to walk toward their source. Use `both(type)` when the question concerns either direction.

For an edge property, traverse the edges themselves:

```gremlin
g.V().hasLabel('Person').has('name', 'alice')
  .outE('ORDERED').values('total')
```

This returns alice's two order totals, 50 and 120.

## Reduce values

```gremlin
g.V().hasLabel('Person').has('name', 'alice')
  .out('ORDERED').values('total').sum()
```

The sum is 170. Use `count()` to count traversers at the current step:

```gremlin
g.V().hasLabel('Person').count()
```

The tutorial contains three people.

## Order and deduplicate

```gremlin
g.V().hasLabel('Person').out('FOLLOWS')
  .values('name').dedup().order()
```

`dedup()` removes repeated values from this projection. `order()` makes the output order explicit. Use `limit(n)` or `range(start, end)` when selecting a window of traversal results.

## Name traversal positions

`as` records a binding for later `select` steps:

```gremlin
g.V().hasLabel('Person').as('person')
  .out('ORDERED').as('order')
  .select('person', 'order')
```

Bindings are useful when the result needs information from multiple stages. For simple application output, project properties directly with `values`.

## Repeated traversal

A bounded `repeat` expresses a repeated graph step:

```gremlin
g.V().hasLabel('Person').has('name', 'alice')
  .repeat(out('FOLLOWS')).times(2).values('name')
```

This asks for people reached after exactly two outgoing steps. Use a bound that matches the domain question, and inspect the [execution plan](/execution.html) when integrating a traversal into an application.
