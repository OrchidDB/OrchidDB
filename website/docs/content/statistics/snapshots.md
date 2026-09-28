# Reusing statistics

A statistics snapshot contains the collected source distributions and the catalog
information needed to associate them with a mapping. Installing it makes those
statistics available to subsequent query compilation.

## In memory

The engine and client libraries retain snapshots in memory. Rust paths share the
snapshot through reference-counted objects. Native bindings use retained catalog
handles, so repeated compilation can reuse a snapshot without serializing its
contents for every query.

A snapshot can serve Cypher, Gremlin and SPARQL queries over the same physical
catalog. Query text, parameters and graph vocabulary can change while the source
statistics remain reusable.

## Across sessions

Snapshots can be saved as JSON and loaded in another session or process. This
separates collection from query serving: collect against the source, persist the
snapshot, then install it where queries are compiled.

Physical schemas and derived source definitions identify the catalog described
by a snapshot. Keep saved snapshots associated with the database and data version
from which they were collected.

## Refreshing

Generate a replacement after substantial changes to row counts, value frequencies,
relationship fanout or collection sizes. Reuse the installed snapshot between
those updates. Queries do not trigger collection or background refresh.

The engine invalidates its cached statistics after writes and transaction
completion. For changes made outside the engine, explicitly regenerate or clear
the snapshot. Clearing it returns planning to the mapping's source metadata and
default choices.

Include the statistics revision in any application-owned compiled-plan cache key.
Installing a different snapshot can change the chosen sources and join order.
