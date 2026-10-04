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
