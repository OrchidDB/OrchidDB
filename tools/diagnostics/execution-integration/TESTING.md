# Local DuckDB extension and Trino/OPA gates

This is internal integration infrastructure, not a release command. It starts an
isolated Docker Compose project named `orchid-execution-integration`, uses fixture
data only, and never publishes or changes clients/production configuration.
README files are not modified.

## Run

From the core repository (`orchiddb/`):

```sh
CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 \
  ORCHID_BUILD_DIR="$PWD/target/diagnostics/integration-extension" \
  python3 extension/scripts/build.py
python3 tools/diagnostics/execution-integration/run.py \
  --extension target/diagnostics/integration-extension/orchid.duckdb_extension
```

Requirements: Docker Compose; the local Rust/C++ extension toolchain; Python with
`extension/tests/requirements.txt` installed (default interpreter
`extension/vendor/test-env/bin/python`, override with `--python`). Network access
is needed for missing Docker images and DuckDB extensions. Missing prerequisites,
failed assertions and skipped required tests fail the run.

Pinned service images: Trino 484, OPA 1.4.2, PostgreSQL 16-bookworm. DuckDB uses the
version pinned by `extension/scripts/build.py` and the test requirements. PostgreSQL is exposed
only at localhost:15439 and Trino at localhost:18089; OPA is internal to the test
network. Fixture passwords and user names are test constants, never credentials
for external services. The runner cannot be pointed at a production endpoint.

The runner retains fixture containers and data for repeat runs. Stop this project
when done (no volume deletion required):

```sh
docker compose -p orchid-execution-integration \
  -f tools/diagnostics/execution-integration/compose.yml stop
```

## What is exercised

### DuckDB UDFs and library bindings

The extension tests verify scalar-only registration, null and numeric boundaries,
and typed Arrow batches. Rust execution tests exercise caller-owned DuckDB
sessions, parameters, transactions, and Arrow results. The Go binding tests run
against the retained module. The extension no longer hosts Cypher or catalogs.

### Trino with OPA

`tests/trino_opa.rs` uses Orchid's actual Rust compiler, plan executor and Trino
source adapter. Tests are explicitly `#[ignore]` in ordinary Cargo runs. This
runner invokes `--ignored` with a required endpoint; missing services cannot count
as successful live coverage.

- PostgreSQL and Iceberg connectors; PostgreSQL-to-memory cross-catalog join fully
  pushed into one Trino query.
- Full source pushdown eliminates DuckDB completion, verified with a completion
  adapter that panics if called.
- Source function/aggregate execution and engine-independent local UDFs.
- Real OPA row filters for Alice/Bob and column masking, admin visibility, denied
  user, and a user allowed to enter Trino but denied `SelectFromColumns`.
- Alternating identities, single-row Arrow batches, exact decimal type/value,
  nulls, empty results, source errors and subsequent session recovery.
- OPA outage injection: stop only this fixture's OPA service, require query failure,
  and restart it in a `finally` block. No permissive fallback is accepted.

Policy configuration follows Trino's official
[OPA integration contract](https://trino.io/docs/current/security/opa-access-control.html).
The Rego is a fixture policy, not a suggested production policy. This local setup
trusts test identity headers; it does not validate TLS, identity-provider login or
impersonation protection in a production deployment.

## Evidence and limits

`target/diagnostics/execution-integration/report.json` records overall and per-suite
status, log paths, extension hash, Git revision and whether the working tree is
dirty. Full stdout/stderr is retained next to it. These gates can exercise pending
engine work; a passing dirty-tree report does not certify the committed release.

The retained live matrix covers Trino with PostgreSQL, Iceberg and memory
connectors, plus local DuckDB UDF and Rust/Go binding checks. It does not establish
production authentication, all deployment platforms, or large-scale performance.
Historical reports describe the scope of the build that produced them.
