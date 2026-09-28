# Statistics

Statistics describe the size and distribution of mapped data so OrchidDB can
estimate how much work a query will do. They help it choose sources, order joins
and filters, and decide how to fetch records during graph traversal. Cypher,
Gremlin and SPARQL all use the same statistics and optimizer.

Statistics are optional. Generate them explicitly, then reuse the resulting
snapshot for subsequent queries.

## When to use statistics

Generate statistics when OrchidDB has meaningful choices to make: several
physical sources for one mapping, joins with very different input sizes,
collections that expand into many rows, or repeated traversals. They are
particularly useful with external data such as Iceberg, where the embedded
database may have little information about the mapped sources.

For simple queries over one source, a database with good statistics can often
plan the SQL well on its own. Collection is less useful there, especially for
one-off queries. Keep the database's optimizer enabled: OrchidDB chooses mapped
sources and shapes the query, then the database optimizes the SQL it receives.

## Generation

Generation reads available source metadata and bounded samples through the
existing database connection. It can make several requests to gather source
sizes, column value distributions, key frequencies, relationship connectivity
and collection lengths. The shared collector produces one snapshot; there are
no statistics tiers or collection settings to tune.

Generate after registering the mappings and loading the data. Collection happens
separately from query compilation, so compiling subsequent queries uses the
snapshot without scanning the data again.

## Optimizer behavior

The optimizer estimates the rows and work involved at each step of a query. It
uses those estimates to:

- Choose among mapped tables, collection sources and SQL definitions containing
  the same logical rows.
- Order connected joins and choose build sides to reduce intermediate results.
- Order eligible filters so selective, inexpensive predicates run earlier.
- Filter collection parents using a selective joined source before expanding
  their elements.

These choices affect the generated SQL. During native graph execution,
statistics also help choose between scanning and caching a source or fetching
records by key, using the number of keys needed by the current traversal.

See the [mapping reference](mapping-reference.md#map-multiple-sources-to-one-table)
for declaring alternative sources.

## Snapshot reuse

Keep a snapshot in the client or engine to reuse it across queries. It can also
be saved and loaded in another session with the same mappings and schemas.
Cypher, Gremlin and SPARQL queries share it; generation is not repeated for each
language or query.

Refresh the snapshot after substantial changes to data volume, value
distributions or collection sizes. Regenerate it when mappings or schemas
change. Loading a saved snapshot avoids collection; generating a new one
replaces the estimates used for subsequent planning.
