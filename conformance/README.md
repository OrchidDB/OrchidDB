# DuckDB extension conformance

| Pinned corpus | Outcome |
| --- | ---: |
| openCypher TCK 2024.3 | 3,897 / 3,897 passed |
| TinkerPop 3.7.4 | 1,511 / 1,511 passed |
| Existing SPARQL 1.0/1.1 baseline | 974 passed; 77 skipped; 74 not applicable |

[Committed evidence](extension-results/summary.json) describes one immutable DuckDB
extension artifact. Compressed full reports retain individual outcomes, catalog
identities, and assertion evidence. SPARQL's preexisting embedded scope is unchanged;
excluded profiles are not counted as passes. This is pinned-corpus conformance,
not certification of every language feature.

Gremlin runs on one extension instance, including the original 15 Java provider
assertions. The adapter does not execute asserted traversals in another graph
engine or merge historical results. Existing JVM algorithms remain compiled kernels.

## Reproduce locally

Build the extension and Python test environment as described in
[verification](../docs/verification.md). Fetch pinned corpora and build the existing
Java assertion adapter and kernel dependencies (Java 21 and Maven required):

```sh
extension/vendor/test-env/bin/python conformance/upstream/fetch.py
extension/vendor/test-env/bin/python conformance/upstream/catalog.py
mvn -q -f jvm-codecs/pom.xml install
mvn -q -f jvm/pom.xml install dependency:build-classpath -Dmdep.outputFile=target/classpath.txt
mvn -q -f conformance/adapters/sqlg/pom.xml package dependency:build-classpath -Dmdep.outputFile=classpath.txt
```

Run suites sequentially against the same unchanged artifact:

```sh
CONFORMANCE_JAVA=/path/to/java extension/vendor/test-env/bin/python extension/conformance/run.py \
  --suite tinkerpop --output target/conformance/extension/gremlin.json
extension/vendor/test-env/bin/python extension/conformance/run.py \
  --suite opencypher --output target/conformance/extension/cypher.json
extension/vendor/test-env/bin/python extension/conformance/run.py \
  --suite rdf --output target/conformance/extension/rdf.json
```

Reports record the artifact SHA-256, DuckDB version, pinned catalogs, adapter and
assertion source identities, and actual case outcomes. Run without filters for a
complete acceptance report. The runner rejects incomplete catalog coverage.

The [historical comparison harness](LEGACY_COMPARISON.md) and `upstream-results/`
remain developer evidence for older library and peer runs. They are not combined
with the current extension outcome. No builds or tests run in GitHub Actions.
