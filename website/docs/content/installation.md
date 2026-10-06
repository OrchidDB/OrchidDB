# Installation

Build from the repository root with Python 3.12+, curl, Rust supporting edition
2024, and a C++17 compiler:

```sh
python3 extension/scripts/build.py
extension/vendor/cli/duckdb -unsigned graph.duckdb \
  -cmd "LOAD 'extension/build/orchid.duckdb_extension'"
```

The script downloads pinned DuckDB 1.5.6 build dependencies and caches them locally.
Use `--release` for optimized Rust code. The artifact is unsigned and tied to its
DuckDB version and platform. Local validation covers macOS ARM64.

Use any matching DuckDB client. For Python:

```python
import duckdb
connection = duckdb.connect('graph.duckdb', config={'allow_unsigned_extensions': True})
connection.execute("LOAD 'extension/build/orchid.duckdb_extension'")
```

Load Orchid before native graph statements. Loading enables DuckDB's parser
extension fallback. No Orchid client package or standalone CLI is needed.
Load source extensions such as Iceberg and Lance on the same connection.
