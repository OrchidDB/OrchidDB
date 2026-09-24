# Native-backed JVM verification

The final local JVM run recorded **1,461 passes, 50 exclusions and no failures,
timeouts or adapter errors** across all 1,511 pinned TinkerPop scenarios. It used
the original Apache `gremlin-test` 3.7.4 assertions, revision
`fa698ba2aba8967dcd17eb61cb13648b934fab5b`. The catalog and immutable parity baseline
were unchanged.

This result measures upstream JVM traversal execution through the production
CrabGraph provider and native Crabgraph store. It does not establish Rust planner
conformance or GraphComputer coverage. The 50 exclusions comprise 31
GraphComputer-only scenarios, 15 original Gherkin placeholders and four scenarios
requiring the opposite null-property policy. The Java provider and GraphComputer
profiles report their original tests separately.

All 28 callback scenarios, three Edge parameter scenarios and two empty-Set
parameter scenarios passed. The remote lambda scenario uses embedded
`RemoteConnection` bytecode submission. Native graph mutations, property identity,
typed values and transaction state remain owned by Crabgraph.

The same final source passed 60 supplemental JVM integration tests with zero
failures or skips. Coverage includes native transactions, competing writers,
GraphMigrator cleanup, cancellation, persistence, callback failures, typed
properties, GraphFactory contracts, services and GraphComputer publication.
The adapter separately passed 23 Java and four Python tests. Supplemental tests
are not added to the upstream scenario count.

| Artifact | Provenance |
| --- | --- |
| JVM source | `4b84efa72228c659efc997d0086082f731bd93bb`, clean before execution |
| JVM jar SHA-256 | `de52da802e8fb334563d4b7491eeead06bb112034fc5137c09fea54e0ba2024d` |
| Combined native source | `1d09c57964dacf17f48dd68c88db4e58ccc1fc27` |
| Native binary SHA-256 | `eb4c4a6ed4f9a15062898c7cdddb7d69b18f4dce6f5c4f467c496f158e45bb91` |
| Full result SHA-256 | `04bdbc7b93f8aa3d740dc489095906d35d4c84d44627050decb64653e3b12a71` |

Local evidence is in `/tmp/gremlin-final-jvm-4b84efa/`: `full1511.json`, its JSONL
stream, `manifest.json`, `verification-summary.json`, frozen classpaths and
`jvm-test-reports/`. The native build manifest is in
`/tmp/gremlin-final-store-integrated-1d09c57/`. These paths describe local artifacts;
publication is a separate integration step.

Earlier diagnostic runs remain preserved. The final candidate fixes all 18
failures and the timeout recorded by the first complete frozen JVM run, with no
passing scenario regressions. The large grateful-graph aggregation now passes
with the original 90-second deadline; its focused run took 12.8 seconds including
fixture loading. Indexed native handle lookup and bounded property records remove
repeated table scans and serialization. No expected result or deadline changed.

See [the production executor guide](gremlin-jvm-executor.md) for build commands,
configuration, transaction ownership and API limitations.
