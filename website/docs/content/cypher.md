Express graph patterns, filter properties, and shape tabular results with Cypher.

## Match a pattern

Cypher uses parentheses for nodes and brackets for relationships. A colon names a label or relationship type, and an arrow gives direction:

```cypher
MATCH (p:Person)-[:ORDERED]->(o:Order)
RETURN p.name, o.total
ORDER BY o.total
```

These queries use the [mapped tutorial dataset](/mapped-graphs.html). The pattern follows each person's orders and returns the person's name with the order total.

## Filter and project

Use `WHERE` to select matches and `AS` to name result columns:

```cypher
MATCH (p:Person)
WHERE p.age >= 30
RETURN p.name AS name, p.age AS age
ORDER BY age DESC
```

The tutorial returns carol, then alice. Use explicit column aliases for stable application-facing result names.

## Read relationship properties

Bind a relationship to a variable to use its properties:

```cypher
MATCH (p:Person)-[r:ORDERED]->(:Order)
WHERE r.total > 100.0
RETURN p.name, r.total
ORDER BY r.total
```

A mapped relationship property resolves to the column configured with `EdgeMapping::property`.

## Aggregate matches

Non-aggregate expressions define the grouping keys:

```cypher
MATCH (p:Person)-[r:ORDERED]->(:Order)
RETURN p.name AS name, sum(r.total) AS spent
ORDER BY name
```

| name | spent |
| --- | --- |
| alice | 170 |
| bob | 80 |
| carol | 500 |

Use `count(...)` to count matches and `DISTINCT` when the query should remove duplicate results.

## Compose query stages

`WITH` introduces an intermediate projection. It is useful for filtering an aggregate before returning the final result:

```cypher
MATCH (p:Person)-[:ORDERED]->(o:Order)
WITH p.name AS name, sum(o.total) AS spent
WHERE spent > 100.0
RETURN name, spent
ORDER BY spent DESC
```

## Preserve optional matches

`OPTIONAL MATCH` keeps the input row when its optional pattern has no match:

```cypher
MATCH (p:Person)
OPTIONAL MATCH (p)-[:FOLLOWS]->(friend:Person)
RETURN p.name, friend.name
ORDER BY p.name, friend.name
```

In the tutorial, carol has no outgoing `FOLLOWS` row, so her optional friend binding is null.

## Bound a traversal

A variable-length relationship can express paths over several hops:

```cypher
MATCH (p:Person)-[:FOLLOWS*1..2]->(friend:Person)
WHERE p.name = 'alice'
RETURN DISTINCT friend.name
ORDER BY friend.name
```

Choose explicit bounds that match the application's question. The distinct projection returns each friend's name once even if several paths reach that friend.

## Parameters and writes

Pass application inputs through `cypher_with_params` and reference them as `$name` or `$minimum`. See [parameters](/parameters.html) for a complete example.

For graph-owned writes, use [managed graphs](/managed-graphs.html). For native updates over mapped tables, use [update properties](/updates.html).
