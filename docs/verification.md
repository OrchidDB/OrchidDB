# Local verification

Build one artifact, then keep it unchanged while running its checks. All builds and
tests run locally. Run commands from the repository root.

```sh
python3 extension/scripts/build.py
python3 -m venv extension/vendor/test-env
extension/vendor/test-env/bin/pip install -r extension/tests/requirements.txt
extension/vendor/test-env/bin/pip install -r conformance/requirements.txt
CARGO_TARGET_DIR="$PWD/target" cargo test --locked --manifest-path extension/compiler/Cargo.toml --lib
PYTHONPATH=extension/tests ORCHID_EXTERNAL_TESTS=1 \
  extension/vendor/test-env/bin/python -m unittest discover -s extension/tests -v
cargo test --locked --features duckdb --lib
cargo test --locked --features duckdb --test foreign_key_relationships --test scalar_primary_keys
```

The 53 extension integrations cover graph DDL, managed state, native syntax,
parameters, Arrow/native values, ordered branches, rollback, prepared execution,
cancellation, and actual Iceberg/Lance storage. There are 5 compiler unit tests,
305 passing core tests (3 existing ignored), and 9 mapped storage/key checks.

For pinned upstream assertions and Java setup, see [conformance](../conformance/README.md).
Current recorded results are 3,897/3,897 Cypher, 1,511/1,511 Gremlin, and 974 existing
SPARQL cases, with 77 skipped and 74 not applicable. Excluded cases are never passes.
Reports retain catalog, assertion, and loaded artifact identities.

Build the documentation separately:

```sh
python3 website/docs/build.py
python3 website/docs/check.py
```

Validation covers macOS ARM64 with DuckDB 1.5.6. Source integration tests are not
cross-platform or distributed transaction certification.
