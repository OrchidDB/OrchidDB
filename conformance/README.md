# Upstream graph conformance comparison

Published report: https://docs.crabgraph.net/conformance.html

Compare **Crabgraph, SQLg and PuppyGraph**, using free editions. The primary
corpus is the original upstream test data and assertions:

| Suite | Pinned version | Scenarios | Compared interfaces |
| --- | --- | ---: | --- |
| openCypher TCK | 2024.3 | 3,897 | Crabgraph, PuppyGraph |
| Apache TinkerPop gremlin-test Gherkin | 3.7.4 | 1,511 | All three |
| W3C SPARQL | SPARQL 1.0 / 1.1 repository revision | 1,125 | Crabgraph |

Every one of the 6,533 scenarios has a recorded outcome for every product.
An absent language interface is not counted as a query failure. The report also
contains 51 sourced capability rows, with paid features marked separately.
This is a compatibility comparison for these versions and profiles, not a
certification or a claim to cover every product feature.

## One Crabgraph execution

The production matrix and leaderboard read `upstream-results/crabgraph-tinkerpop.json`.
Each scenario has one outcome from one suite invocation against one persistent
`GraphEngine`. Typed values and callbacks enter the production Gremlin frontend,
which lowers queries to the SQL IR DAG. DuckDB executes eligible SQL regions;
DataFusion executes residual operators, including JVM callback and graph-computer
kernels. The adapter does not choose executors by scenario, syntax, or prior outcome,
and it never restarts the engine after a crash. Results record the instance identity,
source revision, native binary, adapter classes, and JVM classpath hashes.

Historical provider and GraphComputer runs remain diagnostic evidence. Their passes
are never merged into the product outcome. Gherkin placeholders remain skipped
unless their original assertions execute through this same engine instance.

## Run locally

**Tests run only on the local workstation. GitHub Actions only builds and
publishes static documentation and committed results.**

Requires Python 3.12, Java 21, Maven, Docker Compose, and Rust. These adapters
replace data in their disposable fixture databases; use the dedicated local
containers, not an application database. Run suites sequentially: PuppyGraph
suites share a mapped fixture schema.

```sh
python3.12 -m venv .venv-conformance
. .venv-conformance/bin/activate
pip install -r conformance/requirements.txt
python conformance/upstream/fetch.py
python conformance/upstream/catalog.py
cargo build --bin crabgraph-jvm-store
export CRABGRAPH_JVM_STORE="$PWD/target/debug/crabgraph-jvm-store"
mvn -q -f jvm-codecs/pom.xml install
mvn -q -f jvm/pom.xml install dependency:build-classpath -Dmdep.outputFile=target/classpath.txt
export CRABGRAPH_JVM_CLASSPATH="$PWD/jvm/target/classes:$(cat jvm/target/classpath.txt)"
mvn -q -f conformance/adapters/sqlg/pom.xml package dependency:build-classpath -Dmdep.outputFile=classpath.txt
CARGO_TARGET_DIR="$PWD/target" cargo build --manifest-path conformance/runner/Cargo.toml --bin upstream
docker compose -f conformance/compose.yml up -d
python conformance/wait_ready.py
# PostgreSQL address as seen from the PuppyGraph container:
export PUPPY_JDBC=jdbc:postgresql://postgres:5432/conformance
python conformance/run.py --engine reference --suite tinkerpop
for engine in crabgraph sqlg puppygraph; do
  for suite in opencypher tinkerpop rdf; do
    python conformance/run.py --engine "$engine" --suite "$suite"
  done
done
python -m unittest discover -s conformance/upstream -p 'test_*.py'
python conformance/validate.py
python website/docs/build.py
python website/docs/check.py
```

Set `JAVA_HOME` to Java 21 for Maven; optionally set `CONFORMANCE_JAVA` to the
Java executable. `CONFORMANCE_UPSTREAM_CACHE` overrides the upstream source
cache. `fetch.py` checks the exact immutable revisions and unmodified upstream
trees. Licenses and notices are in `upstream/licenses/`.

For installations using the earlier database container, apply the query bound:

```sh
docker compose -f conformance/compose.yml exec -T postgres psql -U conformance -d conformance -c "ALTER DATABASE conformance SET statement_timeout='10s'"
```

Subset runs require a separate output, keeping the published full run intact:

```sh
python conformance/run.py --engine sqlg --suite tinkerpop --filter Count --output /tmp/sqlg-count.json
```

`--resume` continues an interrupted JSONL journal. Only resume with unchanged
binaries, harness, source pins and runtime configuration. Completed `.json`
artifacts are committed; caches, logs, temporary journals and compiled adapters
are ignored. The static site renders these artifacts without starting engines.

## Assertions and fixtures

- **Gremlin:** Cucumber compiles the original Apache feature files, including
  Scenario Outlines. Unmodified `gremlin-test` `StepDefinition` methods execute
  their assertions. Fixtures come from `TinkerFactory`. The TinkerGraph
  reference run checks the same harness. Standard non-GraphComputer/non-null
  profiles and upstream skips are preserved. PuppyGraph fixture mappings retain
  upstream integer widths; Crabgraph transports native graph objects, paths, numeric widths,
  typed map keys, sets and bulk sets. The JVM structure and
  GraphComputer test suites are outside this Gherkin comparison.
- **Cypher:** the adapter executes original GIVEN/WHEN/THEN steps, result
  tables and side-effect assertions. It preserves duplicates and ordered-result
  requirements. Expected error type/detail/phase must be classified before an
  error assertion can pass: an arbitrary exception is insufficient. PuppyGraph
  fixtures use Neo4j solely to materialize GIVEN statements, then map that graph
  into PostgreSQL. Neo4j supplies no expected answers and is not a compared
  product. PuppyGraph declares openCypher 9; the newer TCK can exercise semantics
  beyond that declared version.
- **SPARQL:** original W3C manifests supply queries, data, named graphs and
  expected artifacts. Syntax tests use the engine parser. Result comparison
  preserves RDF term identity, duplicates, unbound variables and blank-node
  mappings; graph results use RDF isomorphism. Update APIs, protocol tests,
  entailment and remote-service fixtures have explicit applicability outcomes.

Typed graph transport, fixture property types or unavailable service fixtures
can prevent an assertion from being evaluated faithfully. Those outcomes are
recorded separately from semantic failures. The adapters do not rewrite upstream
expectations to match a product. Archived bespoke probes live under `legacy/`
and contribute no primary-suite passes or failures.

## Evidence and timing

Each result includes a stable upstream ID, case hash, outcome, elapsed time,
and available actual output/diagnostic. The catalog preserves original steps,
expectations and pinned source links. Runs record source revisions, runtime
information and engine version or Crabgraph binary hash. The Crabgraph run is a
local working-tree build, not a release benchmark.

Outcomes: `pass`, `fail`, `unsupported`, `skipped`, `not-applicable`,
`adapter-error`, `timeout`. The page additionally detects missing/stale evidence.
A failed assertion is an investigation lead; it can involve an engine, adapter
or version mismatch. Unsupported transport is never silently counted as a pass.

Times are one local execution per scenario, including fixture/adapter work.
Gremlin step and Cypher query measurements are included where available.
Gremlin scenario deadlines are 45 seconds (90 for grateful fixtures), Crabgraph
SQL regions have an 8-second bound, JVM execution has a 30-second query bound,
and the native transport allows 40 seconds to return its result. PostgreSQL statements have a 10-second bound, and
PuppyGraph queries 30 seconds. These are diagnostic timings, not controlled
cross-product performance rankings.

## Updating the report

1. Change source pins deliberately, fetch sources and regenerate the catalog.
2. Rebuild the adapters and run the suites locally against recorded versions.
3. Validate evidence locally; investigate differences using linked cases and
   raw outputs. Keep adapter limitations distinct from product defects.
4. Commit the catalog, source pins and `upstream-results/*.json` with docs changes.
   The publication workflow renders and uploads static files only.

The one-page report supports outcome/suite/search filters and deep links.
Individual evidence panels fetch static JSON files; all rows, source links and
JSON downloads remain available without JavaScript.

## Leaderboard and progress

The static report ranks recorded passes separately for each language, with only
engines that expose that language. `data/parity-baseline.json` preserves the
starting results. The leaderboard shows passed totals and the exact cases
passed by a peer but not by the current engine; its JSON download includes those
case IDs. Updating complete local result artifacts updates the leaderboard during
the next documentation publication. No test suite runs in GitHub Actions.

### Historical Java counterpart diagnostics

This diagnostic adapter is excluded from the product comparison.
The `crabgraph-jvm` adapter executes the 15 pinned Java counterparts when it
encounters Apache's non-executable Gherkin placeholders. Set
`CONFORMANCE_TINKERPOP_SOURCE` to the pinned TinkerPop checkout, or use the
`CONFORMANCE_UPSTREAM_CACHE/tinkerpop` checkout created by `upstream/fetch.py`.
Java 21, Maven, the production provider jar in `CONFORMANCE_GREMLIN_CLASSPATH`,
and `CRABGRAPH_JVM_STORE` are required. The adapter verifies source hashes and
runs the original JUnit methods locally. It records per-test timings, assertion
source and native/provider binary provenance. Failed assertions, assumptions,
and incomplete runs never become passing results. The existing product union
counts each scenario once; supplemental test totals remain separate.
