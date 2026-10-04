# ArcadeDB compatibility harness

Runs the existing pinned openCypher TCK (2024.3) and Apache TinkerPop Gherkin
(3.7.4) assertions against **ArcadeDB 26.9.1**, using temporary embedded databases.
No server, Docker container, application database, or network credentials are used.
The tested queries and expected results are unchanged upstream artifacts.

From the OrchidDB checkout, with Java 21, Maven, and the conformance Python
requirements installed:

```sh
export JAVA_HOME=$(/usr/libexec/java_home -v 21) # macOS; select Java 21 on other systems
export PATH="$JAVA_HOME/bin:$PATH"
export CONFORMANCE_JAVA="$JAVA_HOME/bin/java"
python3 -m venv /tmp/arcadedb-conformance-venv
. /tmp/arcadedb-conformance-venv/bin/activate
pip install -r conformance/requirements.txt
python conformance/upstream/fetch.py
python conformance/upstream/catalog.py
bash conformance/build-arcadedb.sh
export CONFORMANCE_TINKERPOP_SOURCE="$PWD/conformance/upstream/cache/tinkerpop"
CONFORMANCE_PYTHON=python bash conformance/test-arcadedb.sh
python conformance/run.py --engine arcadedb --suite opencypher
python conformance/run.py --engine arcadedb --suite tinkerpop
```

The Gremlin adapter also needs the repository's `orchiddb-jvm` and
`orchiddb-jvm-codecs` Maven artifacts installed, as described in the parent
[conformance guide](../../README.md). These are shared adapter compile dependencies;
ArcadeDB traversals execute directly on `ArcadeGraph`.

Build scripts only compile/package adapters. Tests run locally and explicitly.
Results default to `target/conformance/arcadedb-{suite}.json`, with a JSONL
journal, original case IDs, hashes, timings, actual results, dependency hashes,
Java version, and adapter source hashes. They do not update published evidence.
Subset runs require their own output:

```sh
python conformance/run.py --engine arcadedb --suite opencypher \
  --filter Create1.feature --output target/conformance/arcadedb-create.json
python conformance/run.py --engine arcadedb --suite tinkerpop \
  --filter Count.feature --output target/conformance/arcadedb-count.json
```

## Execution and scope

- Cypher uses ArcadeDB's native embedded `cypher` command interface. Each query
  completes inside a transaction. Each scenario gets a fresh database, including
  schema; GIVEN queries execute on ArcadeDB. The shared TCK runner compares rows,
  duplicates, ordering, graph values, and observed side effects.
- Temporal values preserve dates, nanoseconds, second-resolution offsets, named
  zones, and durations in TCK notation, including temporal properties read back
  from storage. Nodes, relationships, directed paths, nested maps/lists and scalar
  types retain their values. Empty projections, including `RETURN *`, derive
  columns from the provider's own AST and variable scope, without expected tables.
- GIVEN procedures are registered through ArcadeDB's native procedure registry,
  using the original fixture input/output tables and argument counts, and removed
  on reset. The harness does not add query validation to compensate for engine
  behavior.
- Error diagnostics include the complete exception cause chain. `arcadedb_errors.py`
  maps native exception classes and explicit diagnostics from the released source;
  a separate native `EXPLAIN` observes compile/runtime phase. Semantic and syntax
  exception classes stay distinct. An incorrect type, detail or phase fails the
  TCK assertion; exceptions are never classified from the expected error or case ID.
  Unknown future diagnostics remain explicit adapter errors.
- Gremlin executes every traversal through the native provider using a uniform
  **gremlin-groovy / OLTP** profile on ArcadeDB's **TinkerPop 3.8.1 runtime**.
  Typed closures, edge parameters and empty sets enter the provider directly.
  Apache's original 3.7.4 assertion methods still compare the results. The 15
  Gherkin placeholders run their pinned, unmodified original Java counterparts.
  Source hashes and JUnit invocation counts are retained in execution evidence.
- GraphComputer, RemoteOnly and AllowNullPropertyValues cases are excluded by the
  provider/execution profile. Multi/meta-property fixture data unsupported by
  ArcadeDB is reported as unsupported; it is never flattened. ArcadeDB assigns
  element IDs. These product/profile limits do not become passing tests.
- Gremlin output lives in `sqlg/target-arcadedb`, separate from other adapters.
  `ArcadeProviderLoader` isolates the engine's ANTLR 4.13.2 ABI from the original
  assertions' Gremlin parser and its ANTLR 4.9.1 ABI. The provider loader is reused
  across fresh databases, preventing classloader/log-handler accumulation.
  `ArcadeGremlinBootstrap` loads the original 3.7.4 assertion classes with their
  original datetime helper, whose return type changed in 3.8.1. Assertions and
  expected values are not rewritten; provider strategies stay enabled.
- Parent-owned temporary directories are removed on close, including after killed
  or timed-out adapters. Cypher requests have a 20-second transport bound;
  Gremlin uses the shared 45/90-second scenario bounds. RDF is not applicable.

The report includes full coverage for both suites. Full local execution payloads
stay in `target/conformance/`; compact publishable results are under
`conformance/upstream-results/`. The conformance chart and leaderboard read those
recorded artifacts without starting engines. Reproduce using complete, clean
upstream checkouts created by `fetch.py`; exported feature-only caches can omit
required graph fixture files.

`CONFORMANCE_ARCADEDB_CYPHER_CLASSPATH` and
`CONFORMANCE_ARCADEDB_GREMLIN_CLASSPATH` override adapter classpaths; the Gremlin
path must include ArcadeDB 26.9.1 and ANTLR 4.13.2. Java selection uses
`CONFORMANCE_JAVA`. Tests use `CONFORMANCE_PYTHON` when set.

API reference: [ArcadeDB Gremlin documentation](https://docs.arcadedb.com/arcadedb/reference/gremlin/gremlin).

## Reconciliation with ArcadeDB's published Cypher conformance

On 2026-10-04, we reproduced ArcadeDB's published **3,812 passed, 0 failed,
85 skipped (97.8%)** using its unchanged TCK assertion classes, feature files,
fixtures and exclusion list from release `26.9.1`, against the released
`arcadedb-engine:26.9.1` artifact in an isolated Maven module. The suite plugin
was additionally configured to emit Cucumber JSON. Java 21, Cucumber 7.20.1,
JUnit Platform 1.11.4 / Jupiter 5.11.4, and AssertJ 3.27.3 were used. This
reproduces the result without rebuilding the database or running vendor CI.

All 3,897 scenarios were matched by feature, expanded scenario name and
occurrence. Of 220 feature files, 206 are byte-identical to our upstream pin;
the remaining 14 differ only in whitespace. Release age and changed expected
answers therefore do not explain the gap.

| OrchidDB harness outcome | Vendor passes | Vendor skips |
| --- | ---: | ---: |
| Pass | 3,523 | 57 |
| Fail | 289 | 28 |

The 289 scenarios that pass the vendor harness but fail ours break down as:

| Difference | Scenarios | Observed explanation |
| --- | ---: | --- |
| Error diagnostics | 235 | Vendor checks only that an exception occurred; ours checks type, detail and independently observed compile/runtime phase. |
| Side effects | 26 | Label changes move records and change public `id(n)` values. Ours observes added/removed identities and associated property triples; vendor checks net node/relationship/property counts and does not assert label changes. |
| Named graph fixtures | 19 | Original upstream binary-tree fixture queries end in a semicolon, which this engine rejects. Vendor reconstructs the same graphs with queries without the semicolon. Removing only that terminator in an isolated diagnostic run makes all 19 pass our assertions. |
| Path results | 9 | Native outputs are Java lists; ours preserves them as lists. Vendor `pathMatches` accepts any list without inspecting contents. Seven contain alternating node/edge elements; two MERGE cases return node-only lists, including one where an expected relationship is absent. |

The vendor skips 85 scenarios using name-pattern exclusions and unsupported
GIVEN procedures. Our harness executes them: 57 pass and 28 fail. Thus the
232-pass difference is exactly **289 vendor-only passes minus 57 additional
passes in our harness**. It is not evidence of 232 separate database bugs.

The 235 diagnostic cases further split into **105 type-only**, **94 detail-only**,
**14 phase-only**, and **22 type-and-phase** mismatches. The type-only group is
an important limitation in our interpretation: the adapter maps Java exception
classes directly to TCK categories (for example, `CommandSemanticException` to
`SemanticError`). A native exception class name is not itself a documented TCK
error code. These 105 cases have the expected detail and phase, so they must not
be described as 105 demonstrated database correctness bugs. The mapping needs
independent validation against the TCK error taxonomy before attributing those
failures to ArcadeDB. Conversely, the vendor's exception-presence check cannot
establish compliance with the required detail or phase in the other groups.

In particular, the nine path cases include an adapter representation question:
supporting native list-shaped paths would require authoritative provider/query
metadata so ordinary list values remain lists, plus full edge/direction checks.
Seven outputs retain the expected node/edge sequence, making adapter
normalization a plausible cause of those failures; their failure status does
not establish an incorrect traversal. The vendor's unconditional list acceptance
also hides a two-node MERGE output where the relationship needed to verify the
expected path is not returned.
The 26 identity cases expose an observable ArcadeDB storage behavior rather
than a net record-count discrepancy. Error categories are mapped from native
diagnostics, not a vendor claim that all its exception classes implement TCK
error codes. These qualifications matter when interpreting the chart's strict
independent harness outcomes as product behavior.

[Scenario-level reconciliation and input/output hashes](reconciliation.json)
preserve all 317 failures, both harness outcomes, concrete expected/actual
values, the 57 additional passes, and the fixture experiment. Local raw vendor
reports are in `target/conformance/arcadedb-vendor-audit/`; the isolated Maven
module is `/tmp/arcadedb-conformance-reconcile/`. Original chart results were
not replaced by vendor-style relaxed assertions.

Primary sources:
[published compatibility matrix](https://docs.arcadedb.com/arcadedb/reference/cypher/cypher-compatibility),
[released step definitions](https://github.com/ArcadeData/arcadedb/blob/26.9.1/engine/src/test/java/com/arcadedb/query/opencypher/tck/TCKStepDefinitions.java),
[released side-effect checker](https://github.com/ArcadeData/arcadedb/blob/26.9.1/engine/src/test/java/com/arcadedb/query/opencypher/tck/TCKSideEffectChecker.java),
[released result matcher](https://github.com/ArcadeData/arcadedb/blob/26.9.1/engine/src/test/java/com/arcadedb/query/opencypher/tck/TCKResultMatcher.java).
