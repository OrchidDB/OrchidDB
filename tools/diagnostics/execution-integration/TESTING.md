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
version pinned by `extension/scripts/build.py` and the test requirements. Iceberg
and Lance extension identities are printed in the test log. PostgreSQL is exposed
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

### DuckDB extension

- Real PostgreSQL extension scan through registered ontology schema, parameter
  binding, projection/filter/aggregation and decimal source data.
- Real Parquet files plus graph mapping, ICU and JSON functions.
- Existing Iceberg/Lance test: real Iceberg snapshot files, graph join into Lance,
  indexed vector search, and execution-plan evidence of the storage scans.
- Shared helper-function semantics, caller macros/UDFs, lazy conditionals, nested
  values, empty/duplicate groups, correlated subqueries and session isolation.
- Host federation tests: batched requests, caller transactions, cancellation,
  schema-only binding, early results. The HTTP service in those tests is a controlled
  protocol fixture; it is not evidence of a deployed Weaviate installation.
- A separate diagnostic extension build enables the existing Arrow probes. Tests
  verify bounded pulls, replay, early result close, producer errors and exactly-once
  releases. Diagnostic functions are not added to the production artifact.

### Trino with OPA

`tests/trino_opa.rs` uses Orchid's actual Rust compiler, plan executor and Trino
source adapter. Tests are explicitly `#[ignore]` in ordinary Cargo runs. This
runner invokes `--ignored` with a required endpoint; missing services cannot count
as successful live coverage.

- PostgreSQL and Iceberg connectors; PostgreSQL-to-memory cross-catalog join fully
  pushed into one Trino query.
- Both `engine=none` and `engine=duckdb`; no final DuckDB query, verified with a
  completion adapter that panics if called.
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

### Trino → local UDF → DuckDB residual

`test_residual.py` is one embedding test harness using the existing Python wrapper
and freshly built native compiler. It is not a new SDK release matrix or proof of
a complete C execution ABI. The runner stages a matching copy under `target/` with
the current revision pin; production client pin checks are not bypassed or changed.

It verifies that only OPA-authorized rows reach the UDF/completion, DuckDB is created
once for required residual work, Trino receives one query with no local-UDF call,
and `none` rejects the residual before any source or UDF callback executes.

## Evidence and limits

`target/diagnostics/execution-integration/report.json` records overall and per-suite
status, log paths, extension hash, Git revision and whether the working tree is
dirty. Full stdout/stderr is retained next to it. These gates can exercise pending
engine work; a passing dirty-tree report does not certify the committed release.

This matrix covers PostgreSQL, Iceberg, memory, Parquet, Lance, ICU and JSON. It does
not establish live Prometheus/Loki/search-service coverage, S3 permission behavior,
production identity-provider authentication, all deployment platforms, petabyte
scale or million-QPS performance. Those require their own fixtures and evidence
before being advertised as validated. No SDK removal, runtime API migration,
commit, push or publication is performed by this runner.
