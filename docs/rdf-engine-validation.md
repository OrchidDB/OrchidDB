# RDF engine validation

Validated on 2026-09-27 with the shared RDF mappings in the working tree, based on
`5f78f204`. Builds use the development profile. These are correctness checks.
[Machine-readable evidence](rdf-engine-validation.json) includes report hashes
and local log paths.

| Suite | Passing cases |
| --- | ---: |
| openCypher upstream | 3,897 |
| TinkerPop upstream | 1,511 |
| RDF upstream | 974 |
| Targeted core tests | 78 |
| Sibling client integration tests | 96 |

The 6,382 upstream passes match the baseline case IDs, case hashes, and statuses.
RDF retains the existing 74 not-applicable cases and 77 skips. Expectations and
exclusions were not changed. The RDF runner now uses GraphEngine.

The ordinary-table regression tests cover composite and binary identities,
Unicode encoding, native-key joins, predicate pruning, typed literals, paths,
variable predicates, named graphs, empty graph registries, and SQL views. Write
tests cover shared Cypher visibility, explicit rollback, failed transactions,
required-column replacement, defaults, inverse FKs, insertion dependency order,
and the absence of hidden storage tables.

The featureless compiler also passes its 10 SQL boundary tests. The existing
Rust, Java, Python, JavaScript, Elixir, CLI, and C++ suites and examples use freshly
built local compiler artifacts. New Python, JavaScript, and Java cases execute
RDF rules against their caller-owned application tables.

The documentation build and checker pass for 25 chapters, 29 HTML documents,
and 55,957 local links/assets.
