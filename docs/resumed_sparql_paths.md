# SPARQL paths and graph results: resumed S3 work

Resumed the interrupted S3 assignment on 2026-09-23. The earlier Claude worker
had investigated the code but had not implemented paths.

## Implementation

`src/ir/rel/rdf_paths.rs` lowers typed endpoint relations for predicate,
inverse, sequence, alternative, negated-set, optional, star and plus paths.
Closures use recursive SQL with UNION distinct, so cycles terminate on endpoint
pairs without counting duplicate walks. Sequence and alternative retain bag
semantics; negated sets and closure operators deduplicate their own endpoints.
This follows the [SPARQL path evaluation rules](https://www.w3.org/TR/sparql11-query/#defn_evalPP).

Path identity includes lexical value, kind, datatype and language. Paths remain
inside the active graph; FROM merges and FROM NAMED restrictions use the existing
quad-source layer. Zero-length paths include subjects and objects, plus constant
endpoints even outside the graph. Fixed or variable named-graph scopes require a
represented graph. EXISTS can seed zero-length identity from bound outer terms.
Explicit VARCHAR casts preserve recursive metadata types when initial values
are NULL and later steps reach literals.

## DESCRIBE policy

The frontend lowers DESCRIBE to graph construction of **outgoing triples of the
selected resources in the query's default graph**, including FROM merges. It
does not recursively expand blank-node objects. WHERE, ordering and slicing
select resources before describing them. Unbound resources contribute no triples.
The low-level standalone GraphDescribe IR node still has no relational lowering;
queries through SparqlPlanner use the executable construction plan instead.

CONSTRUCT uses a materialized UUID namespace and row identity per solution for
template blank nodes. Repeated template references share a node within a solution;
duplicate solutions get distinct nodes. Graph output remains a set of triples.

## Validation and limitations

Focused expected-result tests live in `tests/sparql_property_paths.rs`: composed
paths, typed and absent zero-length endpoints, named/default graph isolation,
cyclic and nested closure, outer bag multiplicity, negated-set deduplication,
missing/excluded named graphs, correlated EXISTS, DESCRIBE and fresh template
blank nodes. The graph test checks structure without depending on blank labels.

The mapping has no explicit empty-named-graph registry: graph existence is inferred
from quad rows, so independently registered empty named graphs remain unmodeled.
No W3C manifest sweep was run. This work does not complete federation (S4), all
SPARQL function/precision gaps, or the full completion plan's conformance gate.

The eight focused path/result tests pass. Mixed bound/unbound EXISTS rows and
EXISTS result marks have separate regressions: outer row keys partition all path
relations, including recursive DISTINCT, and remain attached through final
endpoint binding. Existing SPARQL/RDF/mapped-engine regression targets also pass.
