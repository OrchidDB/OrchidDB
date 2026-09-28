# Statistics

Statistics describe the data behind a graph mapping: how many rows a source
contains, which values are common, how keys are distributed, and how much data
collections expand into. OrchidDB uses this information to choose physical
sources, order joins, and select access methods.

The same statistics serve Cypher, Gremlin and SPARQL. They belong to the mapped
sources, so one collection can support many queries across all three languages.

Statistics are optional. Generate a snapshot explicitly, then reuse it during
planning. Without a snapshot, OrchidDB uses the mapping definitions and supplied
source metadata. With a snapshot, the optimizer also uses the collected data
distributions. There are no collection profiles or tuning tiers.

## Collection, planning and reuse

[Generation](statistics/generation.md) reads source metadata and bounded samples
through the application's existing database session. The shared collector turns
those reads into a snapshot of source and column statistics.

[The optimizer](statistics/optimization.md) uses the snapshot when compiling a
query. It compares the work involved in reading sources and producing intermediate
rows. Native graph execution also uses it when choosing how to fetch a traversal's
next set of records.

[Snapshots](statistics/snapshots.md) can remain in memory or be saved and loaded
across sessions. Queries reuse the installed snapshot; generation is a separate
operation.

## Where statistics matter

OrchidDB makes choices that span graph mappings and SQL execution. A logical
source can have several physical representations. A graph traversal can issue
multiple database requests. Statistics help OrchidDB choose the representation
or access method before handing work to the database.

Start with [when to use statistics](statistics/when-to-use.md) to decide whether
collection is worthwhile for your workload.
