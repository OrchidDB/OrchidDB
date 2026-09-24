# Gremlin final-gap implementation results

## Execution architecture

Crabgraph now provides a production JVM traversal executor backed by its native
`PropertyGraph` store. The JVM provider implements TinkerPop graph interfaces;
the Rust store owns graph state, identity, mutations and persistence. Groovy and
bytecode traversal submissions execute through this provider. GraphComputer
provides single-process supersteps, messaging, memory reduction and result graph
publication over the same native provider.

These are explicit execution profiles. Ordinary queries through the native Rust
planner retain their own result artifact. This change does not implement an
automatic mixed SQL/interpreter/JVM DAG or JVM-to-DuckDB re-entry.

The optional `jvm-codecs` module handles GraphML/Gryo format conversion. Its
TinkerGraph staging is a format codec; the production traversal executor uses
Crabgraph native graph state.

## Original gap inventory

All **84 originally identified non-passing scenarios** have passing behavioral
evidence:

- **69 executable scenarios** pass their original upstream assertions in the
  appropriate native, JVM OLTP or GraphComputer profile.
- **15 literal Gherkin placeholders** retain their original skipped status.
  Their corresponding pinned upstream Java tests all pass separately.

The [case-to-evidence mapping](../conformance/upstream-results/gremlin-final-gap-evidence.json)
records every original gap, its baseline status, each executed profile and the
corresponding Java method where applicable. It does not synthesize passing
Gherkin results from Java tests.

| Execution profile | Passed | Skipped | Unsupported | Failures / timeouts |
| --- | ---: | ---: | ---: | ---: |
| Native Rust, all 1,511 Gherkin cases | 1,432 | 78 | 1 | 0 |
| JVM OLTP, all 1,511 Gherkin cases | 1,461 | 50 | 0 | 0 |
| GraphComputer, 31 selected algorithm Gherkin cases | 31 | 0 | 0 | 0 |
| Original Java methods corresponding to 15 placeholders | 15 | 0 | 0 | 0 |

The native run preserves all 1,427 previous passes and adds the five original
null-property cases. JVM OLTP excludes 31 GraphComputer cases, 15 literal upstream
placeholders and four cases requiring the opposite null policy. These policies
are separate tested configurations; their counts are not combined into a single
profile's pass total.

Raw evidence:
[native](../conformance/upstream-results/crabgraph-tinkerpop.json),
[JVM OLTP](../conformance/upstream-results/crabgraph-jvm-tinkerpop.json),
[GraphComputer](../conformance/upstream-results/crabgraph-computer-tinkerpop.json),
[original Java placeholder tests](../conformance/upstream-results/java-provider/placeholder-java-tests.json).

## Implemented behavior

- Native vertex/edge/property identity and typed transport, including nested
  maps, empty sets, arbitrary-precision values and callback submissions.
- Lambda execution, sack callbacks and the original inline remote-lambda case
  through the JVM executor's embedded remote connection.
- Connected components, PageRank, peer pressure and weighted shortest paths
  computed from graph data through GraphComputer.
- Stored-null and null-as-removal configurations, property cardinality and
  typed dynamic cardinality arguments for native merge operations.
- Transaction ownership, committed-state reads from independent threads,
  rollback, savepoints, cancellation and persistent store lifecycle. Bounded
  read caches invalidate on mutation and rollback; foreign committed reads
  bypass caches belonging to a writer.
- Native file writers and independent reader round trips through production
  codecs, with atomic destination publication.
- Cyclic project modulators, seeded group counts, bounded lazy consumption,
  map-key/null behavior, and count/fold group writers finalized by explicit cap.

See [JVM executor contracts](gremlin-jvm-executor.md),
[GraphComputer contracts](gremlin-graphcomputer.md),
[dynamic cardinality](gremlin-dynamic-cardinality.md), and the internal
[native evaluator scope](gremlin-native-semantic-backlog.md).

## Reproducibility and broader suites

The upstream TinkerPop 3.7.4 catalog, assertions and saved comparison baseline
are unchanged. Results record source revisions, binary/JAR hashes and the
original scenario identifiers. Source and executable identity are captured
before execution. The native and JVM build manifests accompany the artifacts.

The complete original `ProcessComputerSuite` records **700 passed and 168
upstream exclusions across 868 test occurrences**, with zero failures or
timeouts. Those occurrences cover 852 unique tests (695 passed, 157 excluded);
repeated upstream suite entries remain visible. All 37 computer contracts and
10 interruption tests pass.

The broader original Java suites and supplemental contracts have independent
denominators and execution profiles. Their recorded outcomes are listed in the
[Java evidence index](../conformance/upstream-results/java-provider/index.json).
Supplemental tests are identified separately from original upstream tests.

All tests run locally. GitHub Actions builds and publishes only the static
documentation and committed evidence. Timings in the artifacts include fixture,
adapter and execution work; they are not a controlled performance comparison.
