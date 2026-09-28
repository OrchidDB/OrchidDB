# When to use statistics

Generate statistics when they can change how OrchidDB accesses the data. The
strongest opportunities are choosing among physical representations, reducing
large intermediate joins, and selecting access methods for native traversal.

## Multiple ways to read the same logical data

Statistics are particularly useful when a mapping offers equivalent flat,
nested or materialized sources. OrchidDB chooses which source appears in the SQL;
the database optimizes the statement it receives. A snapshot gives OrchidDB the
information to compare those alternatives in the context of the whole query.

## External data and substantial joins

For embedded queries over external sources such as Iceberg, collected distributions
can help when the database has little information about filters and join keys.
Starting from a selective relation can avoid producing a large intermediate join.

Keep the database's optimizer enabled. OrchidDB's source choices and graph planning
complement the database's physical planning. A backend with good statistics may
already choose an effective order for a straightforward SQL join.

## Native traversal work

Statistics help when a traversal requires several fetches from the database.
OrchidDB sees the frontier and the sequence of graph operations, which lets it
compare repeated key batches with a source scan that can be cached and reused.

## Repeated queries over stable data

Collection pays for itself through reuse. A mapping that serves many queries over
a stable dataset is a good candidate: generate a snapshot once and use it for
subsequent planning. An expensive one-off query can also justify collection when
a different access path saves substantially more work than collection requires.

Leave statistics off for cheap queries with one obvious source and a backend that
already plans them well. They are also less attractive when a short-lived session
or rapidly changing dataset leaves little opportunity to reuse the snapshot.

The decision is whether the expected execution savings justify collection and
refresh. Start with the queries that read the most data or produce the largest
intermediate results.
