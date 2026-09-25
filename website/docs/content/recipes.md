# Query recipes

Answer common relationship questions with the customer and order graph from the mapped tutorial.

## Dataset

These recipes use the [mapped graph tutorial](mapped-graphs.md): three people, four orders, and three `FOLLOWS` relationships. Use lowercase names to match the sample rows.

## Find a person's friends

```cypher
MATCH (p:Person)-[:FOLLOWS]->(friend:Person)
WHERE p.name = 'alice'
RETURN friend.name AS name
ORDER BY name
```

The names are bob and carol. The equivalent Gremlin traversal is:

```gremlin
g.V().hasLabel('Person').has('name','alice')
  .out('FOLLOWS').values('name').order()
```

## Find customers with large orders

```cypher
MATCH (p:Person)-[:ORDERED]->(o:Order)
WHERE o.total > 100.0
RETURN p.name AS name, o.total AS total
ORDER BY total
```

The results are alice with 120 and carol with 500.

## Sum spending by customer

```cypher
MATCH (p:Person)-[r:ORDERED]->(:Order)
RETURN p.name AS name, sum(r.total) AS spent
ORDER BY spent DESC
```

| name | spent |
| --- | --- |
| carol | 500 |
| alice | 170 |
| bob | 80 |

For one customer, use a traversal reduction:

```gremlin
g.V().hasLabel('Person').has('name','alice')
  .out('ORDERED').values('total').sum()
```

## Find friends within two hops

```cypher
MATCH (p:Person)-[:FOLLOWS*1..2]->(friend:Person)
WHERE p.name = 'alice'
RETURN DISTINCT friend.name AS name
ORDER BY name
```

Alice reaches bob directly and carol both directly and through bob. `DISTINCT` produces one row per name.

## Count the people

```gremlin
g.V().hasLabel('Person').count()
```

Or use Cypher:

```cypher
MATCH (p:Person)
RETURN count(p) AS people
```

Both return three for the sample dataset.

## Filter through an RDF vocabulary

With the [SPARQL ontology](sparql.md#define-a-vocabulary):

```sparql
PREFIX ex: <https://example.com/>
SELECT ?name ?age WHERE {
  ?p a ex:Person ; ex:name ?name ; ex:age ?age .
  FILTER(?age > 29)
}
ORDER BY ?name
```

The names are alice and carol, with ages 30 and 41.

## Apply a property update

Run through `MappedGraphEngine::cypher_update`:

```cypher
MATCH (p:Person)
WHERE p.name = 'alice'
SET p.age = 31
```

Query `p.age` afterward to read the updated value. Use [typed parameters](parameters.md) when the name or new age comes from user input.
