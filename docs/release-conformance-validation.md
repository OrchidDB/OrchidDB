# Release conformance validation

The final run passes all 6,382 applicable cases: 3,897 openCypher, 1,511 TinkerPop, and 974 RDF. All case definitions and outcomes match the prior baseline, including 74 not-applicable and 77 skipped RDF cases. It uses optimized release binaries and the unchanged upstream assertions. Cypher state snapshots, immutable Gremlin fixture checkpoints, and engine-owned JVM workers reduce repeated setup and observation overhead. Tested-query results are not cached. RDF received no dedicated harness optimization.

The initial complete release run exposed 29 semantic/SQL failures. The fixes preserve declared correlated outputs, enforce labels on bound endpoints, preserve limit and UNNEST SQL scopes, share correlation row identities, retain numeric types across null injection, and preserve step labels during repeat history elision. A subsequent run exposed three RDF ordering failures; restricting the limit projection repair to joins preserves DISTINCT ordering before OFFSET/LIMIT, including subqueries. Focused tests cover returned values and SQL boundary costs; the final complete run validates the original cases again.

Timing compares one local run per case with the earlier development-build evidence. Release optimization, lowering improvements, and harness changes all contribute; these results do not isolate their individual effects or establish cross-product rankings.

Only compact reports are published. Full requests and returned payloads remain in ignored target files. The machine-readable release-conformance-validation.json records the source revision, binary hashes, unchanged case definitions/outcomes, timing summaries, and cost comparisons.

## Recorded case times

| Suite | Earlier mean | Release mean | Release median |
| --- | ---: | ---: | ---: |
| Cypher | 91.98 ms | 4.47 ms | 2.14 ms |
| Gremlin | 128.24 ms | 20.75 ms | 6.50 ms |
| RDF | 60.52 ms | 9.54 ms | 8.27 ms |

Source: `0be57f2d7a7d49286bfbb2df992c9d6a730fb147`. See [validation metadata](release-conformance-validation.json) and the [published report](https://docs.orchiddb.com/conformance-report.html#query-cost).
