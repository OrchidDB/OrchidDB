# Native-backed JVM verification

The final local JVM run recorded **1,461 passes, 50 exclusions and no failures,
timeouts or adapter errors** across all 1,511 pinned TinkerPop scenarios. It used
the original Apache `gremlin-test` 3.7.4 assertions, revision
`fa698ba2aba8967dcd17eb61cb13648b934fab5b`. The catalog and immutable parity baseline
were unchanged.

This result measures upstream JVM traversal execution through the production
CrabGraph provider and native OrchidDB store. It does not establish Rust planner
conformance or GraphComputer coverage. The 50 exclusions comprise 31
GraphComputer-only scenarios, 15 original Gherkin placeholders and four scenarios
requiring the opposite null-property policy. The Java provider and GraphComputer
profiles report their original tests separately.

All 28 callback scenarios, three Edge parameter scenarios and two empty-Set
parameter scenarios passed. The remote lambda scenario uses embedded
`RemoteConnection` bytecode submission. Native graph mutations, property identity,
typed values and transaction state remain owned by OrchidDB.

The same final source passed 63 supplemental JVM integration tests with zero
failures or skips. Coverage includes native transactions, competing writers, committed readers,
GraphMigrator cleanup, cancellation, persistence, callback failures, typed
properties, GraphFactory contracts, services and GraphComputer publication.
The adapter separately passed 23 Java and four Python tests. Supplemental tests
are not added to the upstream scenario count.

| Artifact | Provenance |
| --- | --- |
| JVM source | `cbd8fa37e26a76a723d76df27abb04a2474f5820`, clean before execution |
| JVM jar SHA-256 | `ca4cd84c245611bc0edcbdc1ef197a3cf43df70b7ecadea63f66b638ea0cf468` |
| Combined native source | `2ae5e7529a9054be57a853759ec61100f9f1b811` |
| Native binary SHA-256 | `7aaa3f297a35d457b5c9c7eec23de4e157af65b07d90bc59280292c2c1bb7354` |
| Full result SHA-256 | `982f3d9e788a906641c4c82aa2725a9a88d6f909771472b7fc97eb41b7a22001` |

Local evidence is in `/tmp/gremlin-final-jvm-cbd8fa3/`: `full1511.json`, its JSONL
stream, `manifest.json`, `verification-summary.json`, frozen classpaths and
`jvm-test-reports/`. The native build manifest is in
`/tmp/gremlin-committed-read-native-2ae5e75/`. These paths describe local artifacts;
publication is a separate integration step.

Earlier diagnostic runs remain preserved. The final candidate fixes all 18
failures and the timeout recorded by the first complete frozen JVM run, with no
passing scenario regressions. The large grateful-graph aggregation now passes
with the original 90-second deadline; its final full-suite run took 12.0 seconds including
fixture loading. Indexed native handle lookup and bounded property records remove
repeated table scans and serialization. No expected result or deadline changed.

The final pair also passed all 29 targeted original Java tests: seven
multithreaded transaction tests, 12 element-ID strategy tests and 10 traversal
interruption tests. These are separate Java evidence, not additional Gherkin
passes. Both broad provider suites completed on the same frozen jar and native
binary with zero failures: ProcessStandardSuite recorded 920 passes and 17
exclusions across 937 invocations; StructureStandardSuite recorded 656 passes and
287 exclusions across 943 invocations (831 unique test IDs). These counts retain
upstream parameterized invocations and feature exclusions. Their source, harness
and result hashes are indexed in
`/tmp/gremlin-provider-final-evidence-manifest.json` and published as separate Java
evidence.

See [the production executor guide](gremlin-jvm-executor.md) for build commands,
configuration, transaction ownership and API limitations.
