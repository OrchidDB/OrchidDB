# How statistics change planning

OrchidDB estimates the rows and work produced by relational operations, then uses
those estimates to compare plans. Filters, joins, grouping, duplicate elimination
and collection expansion all contribute to the comparison.

## Choosing physical sources

A logical source can map to several equivalent representations: a flat table, a
collection expansion, a query-defined relation or a materialized result. Statistics
let OrchidDB compare the scans and intermediate operations needed by each one.

Selection considers the surrounding query. A representation that is cheap to scan
on its own can become expensive when it produces many rows for a later join.
Conversely, a selective neighboring relation can make a nested representation
attractive by reducing the parents that need expansion.

For equivalent physical layouts, collected source costs complement supplied
partition metadata. Partition and file costs remain part of source selection.

## Ordering connected joins

The optimizer compares orders for connected inner joins. A selective relation can
join early, reducing the rows passed to later operations. Key frequencies and
distinct tuple counts help estimate the size of each intermediate result.

The comparison also accounts for the side used to build a hash table. Source
selection and join ordering work together: representation alternatives are scored
after considering the connected plan.

The selected structure is expressed in the generated SQL. The database then
performs its own physical optimization.

## Filtering parents before collection expansion

When a join relates collection elements to another source through a parent key,
OrchidDB can use the neighboring key relation to filter parents before `UNNEST`.
This reduces the number of collection elements produced for the rest of the query.

The original binding join remains in the plan. It supplies the requested bindings
and preserves the multiplicity of matching rows. The parent filter uses keys from
the query's runtime relation.

## Choosing native access

Native graph operations fetch records and neighbors for a frontier: the set of
identities currently being traversed. OrchidDB compares batched key lookups with
scanning a source into a statement-local cache.

The choice combines collected source costs with the actual number of distinct
frontier keys. Sparse frontiers favor targeted batches. A dense frontier can make
one scan worthwhile, particularly when subsequent traversal steps can reuse the
cached records. The same cache supports incoming and outgoing edge access.

## Other planning uses

Collected selectivity guides the order of supported pure filter comparisons.
Null frequencies, value distributions and tuple statistics feed row estimates
through joins and grouping. Collection lengths feed expansion estimates. These
estimates are shared by the graph-language frontends and relational planning.
