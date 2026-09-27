# RDF engine unification

RDF vocabulary is now part of `GraphMapping`. `RdfMapping` constructs terms over
registered application tables or SQL views. The typed SPARQL algebra lowers those
rules to relational scans, filters, joins, and projections; no triple storage is
created. Constant predicates prune branches, and compatible single-rule identity
joins compare native composite-key components.

`GraphEngine` owns the connection and transactions for all three languages.
`RdfGraphEngine` delegates to it. RDF execution contexts borrow the common DAG
session. Typed results retain RDF identity metadata and execution cost reporting.

Writable rules invert terms into physical row effects. Inserts coalesce by key,
DELETE/INSERT avoids transient nulls, and database FK dependencies order effects.
The common transaction lease restores the connection and rolls back canceled
automatic updates. Read-only, ambiguous, conflicting, or incomplete writes fail
atomically.

Compiler JSON accepts the same rules. Existing mapped ontology builders adapt to
rules and retain repeated predicate declarations. The upstream RDF adapter uses
GraphEngine; application-owned quad fixtures remain one supported mapping shape.

The RDF guide documents the public contract. `tests/rdf_relational.rs` exercises
ordinary-table joins, composite identity, paths, graph scopes, typed literals,
updates, cross-language visibility, rollback, and absence of hidden tables.
