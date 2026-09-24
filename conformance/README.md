# Graph engine comparison

Static report: https://docs.crabgraph.net/conformance.html

## Scope

Five free editions, 194 exact query probes, 72 sourced capability rows, and an
inventory of all 7,302 imported Ladybug/TinkerPop cases. These are separate evidence
sets. Imported cases are not counted as cross-product passes. A query error does
not prove that the product cannot express the operation in another dialect.
The probes do not certify complete language conformance or benchmark performance.

All 73 Gremlin expectations are checked against TinkerGraph 3.7.4. Neo4j is
explicitly queried in Cypher 5 mode. The fixture is four people and four KNOWS
edges, including a cycle and an absent property. SQLg uses PostgreSQL 15;
PuppyGraph maps a separate PostgreSQL schema. Ladybug declares typed tables.
Mutating probes use rollback for Crabgraph, Ladybug, Neo4j and SQLg.
PuppyGraph exposes read queries against the fixture.

## Reproduce

Requires Python 3.12, Java 21, Maven, Docker Compose and Rust (for Crabgraph).
These adapters **initialize and delete data** in their disposable fixture databases.
Use only the dedicated containers and test credentials below. Do not point them
at an existing application database.

```sh
python3.12 -m venv .venv-conformance
. .venv-conformance/bin/activate
pip install -r conformance/requirements.txt
mvn -q -f conformance/adapters/sqlg/pom.xml package dependency:build-classpath -Dmdep.outputFile=classpath.txt
docker compose -f conformance/compose.yml up -d
python conformance/wait_ready.py
PUPPY_JDBC=jdbc:postgresql://postgres:5432/conformance python conformance/setup_puppy.py
python conformance/catalog.py
for engine in reference ladybug neo4j puppygraph sqlg; do
  python conformance/run.py --engine "$engine"
done
CARGO_TARGET_DIR="$PWD/target" cargo build --locked --manifest-path conformance/runner/Cargo.toml
python conformance/run.py --engine crabgraph
python conformance/validate.py
python website/docs/build.py
python website/docs/check.py
docker compose -f conformance/compose.yml down -v
```

Use `CONFORMANCE_JAVA=/path/to/java` if Java 21 is not the default. Use
`CRABGRAPH_CONFORMANCE_BIN=/path/to/binary` to select a built Crabgraph adapter.
`CONFORMANCE_READ_MODE=sql-only` runs the Crabgraph SQL-only variant; save it with
`--output /tmp/crabgraph-sql-only.json` so the hybrid snapshot is preserved.
`--filter cypher.paths` selects matching probe IDs. Filtered output must be saved
separately; validation rejects incomplete snapshots.

## Evidence format

- `catalog.py` → `probes.json`: fixture, query, expected rows, order and mutation flags.
- `results/*.json`: version, timestamps, probe hashes, actual rows/errors, elapsed
  adapter time and Crabgraph backend. The initial Crabgraph snapshot is a modified
  local repository build; its binary SHA-256 is recorded. It is not a released build.
- `capabilities.py` → `data/capabilities.json`: reviewed availability, primary source,
  review date, paid edition / extension / documented-unavailable classifications.
- `inventory.py` → `data/inventory.json`: every imported case, group, source path,
  dataset and content digest. Inventory generation does not execute those cases.
- `validate.py`: requires complete adapter coverage, current probe hashes, no
  harness failures, and a passing TinkerGraph reference run.

Each probe runs three consecutive times by default (`--repetitions` changes this).
The report shows median client wall time and exports minimum, maximum and every
sample. The first run is included; no warmup is performed. Network and embedded
adapter overhead differs. This tiny fixture is not a scalability benchmark.
`data/investigations.json` records semantic review separately from observations.
A mismatch is not automatically a product defect.

Results distinguish pass, different rows, query rejection, timeout and harness
failure. The comparator preserves row multiplicity, nulls, booleans and nested list
order. Unordered top-level results use multiset equality. Numeric comparisons use
relative and absolute tolerance 1e-9. Graph elements and unsupported Arrow values
are not silently coerced to primitive matches. A killed/timed-out adapter cannot
supply delayed output to a later probe.

## Updating the published comparison

Run the suite locally using the commands above. Review result differences, then
commit the result JSON and any accompanying investigation notes. GitHub Actions
only builds and publishes the static sites from committed evidence; it does not
run the conformance engines. Publication uses the existing personal AWS secrets.

The Crabgraph snapshot records a local modified build and its binary hash. Do not
relabel it as a released build. Changed probes invalidate old snapshots and must
be rerun locally before publishing. Run `python conformance/investigate.py` to
reproduce the SQLg optimizer investigation against the disposable local database.

Add an independently justified expectation with each new test. Check Gremlin
against the reference; use standards examples or manually derived Cypher/SPARQL
results. Add dialect equivalents as separate probes, with an explanation. Expand
fixtures for constraints, temporal/spatial values, RDF datasets, recovery,
concurrency and operational capabilities. Edition claims require manual source
review on upgrades. Never turn an unassessed cell into an unsupported claim.
