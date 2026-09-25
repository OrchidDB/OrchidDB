# Installation

Use the CLI for DuckDB and Iceberg, or embed a language client around your own engine. Source builds are available. Registry packages and binary releases are not published yet.

## From source

The standalone CLI lives in [OrchidDB-cli](https://github.com/OrchidDB/OrchidDB-cli):

```sh
git clone https://github.com/OrchidDB/OrchidDB-cli.git
cd OrchidDB-cli
cargo install --locked --path . --bin orchiddb
orchiddb query examples/people.json --init examples/setup.sql --format table
```

Use Rust with edition 2024 support, Git, and a C/C++ build toolchain. The first build compiles bundled DuckDB. Add Cargo's executable directory (usually `$HOME/.cargo/bin`) to `PATH`.

Queries load the official Iceberg extension by default. First use requires network access to install the extension. Your setup SQL configures catalogs, credentials, views, plugins, and UDFs. See the [quickstart](quickstart.md) and [CLI reference](cli.md).

## Binary installer

After the first published CLI release:

```sh
curl -fsSL https://install.orchiddb.com | bash
```

The installer downloads from [OrchidDB-cli releases](https://github.com/OrchidDB/OrchidDB-cli/releases), verifies SHA-256 checksums, and atomically installs into `~/.local/bin`. It never runs sudo or edits your shell configuration. With no published release it fails with source-build instructions and leaves an existing installation unchanged.

Release packaging targets macOS ARM64/Intel and Linux x86_64 (glibc 2.35+). Windows, Linux ARM64, and musl CLI binaries are not packaged. macOS binaries are not signed or notarized.

```sh
curl -fsSL https://install.orchiddb.com | ORCHIDDB_VERSION=v0.1.0 ORCHIDDB_INSTALL_DIR="$HOME/.local/bin" bash
```

Without a version, the installer selects the most recently created published release, including prereleases. [Review the installer](https://install.orchiddb.com/) before running it. Checksums come from the same release publisher as the archive.

## Language clients

| Client | Source | Result interface |
| --- | --- | --- |
| Rust | [OrchidDB-rust](https://github.com/OrchidDB/OrchidDB-rust) | Arrow `RecordBatchReader` |
| Python | [OrchidDB-python](https://github.com/OrchidDB/OrchidDB-python) | PyArrow reader |
| JavaScript / TypeScript | [OrchidDB-js](https://github.com/OrchidDB/OrchidDB-js) | Async Arrow batches |
| Java | [OrchidDB-java](https://github.com/OrchidDB/OrchidDB-java) | Arrow vectors / `ArrowResult` |
| Elixir | [OrchidDB-elixir](https://github.com/OrchidDB/OrchidDB-elixir) | ADBC Arrow C Stream callback |
| C++ | [OrchidDB-cpp](https://github.com/OrchidDB/OrchidDB-cpp) | Arrow C stream with RAII ownership |

Start with [client setup and runnable examples](client-apis.md). These clients compile SQL without a DuckDB driver dependency. Your application supplies the engine. Rust uses a Git dependency; Java builds its JNI compiler; the other bindings use the shared native compiler below.

## Shared native compiler

Python, Node.js, Elixir, and C++ source builds need [OrchidDB-native](https://github.com/OrchidDB/OrchidDB-native). Check out the revision in your client's `NATIVE_REVISION`, then the compiler's matching core revision. Run this from the client repository root (requires new sibling checkout directories):

```sh
git clone https://github.com/OrchidDB/OrchidDB-native.git ../orchiddb-native
git -C ../orchiddb-native checkout "$(cat NATIVE_REVISION)"
git clone https://github.com/OrchidDB/OrchidDB.git ../orchiddb
git -C ../orchiddb checkout "$(cat ../orchiddb-native/CORE_REVISION)"
cargo build --locked --release --manifest-path ../orchiddb-native/Cargo.toml
```

Set the path for the current shell, then run your client's example:

```sh
# macOS; use liborchiddb_compiler.so on Linux
export ORCHIDDB_NATIVE_LIBRARY="$(cd ../orchiddb-native/target/release && pwd)/liborchiddb_compiler.dylib"
```

The library compiles metadata and query text only; result data never crosses this boundary. Release workflows bundle it into Python wheels, npm packages, and C++ archives. Elixir loads it explicitly. No binding downloads a compiler implicitly at runtime.

## Core compiler library

For direct compiler development:

```toml
[dependencies]
orchiddb = { git = "https://github.com/OrchidDB/OrchidDB.git", default-features = false }
```

See [SQL compilation](sql-compiler.md). For application integration, prefer the [Rust client](client-apis.md#rust).

<a id="use-the-rust-library"></a>

## Optional managed Rust runtime

The older managed engine APIs remain available in core. Their tutorials use an explicit `duckdb` feature and have different capabilities from compiler-only clients:

```toml
[dependencies]
orchiddb = { path = "../orchiddb", features = ["duckdb"] }
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
arrow = "58.2.0"
```

Run its separate example from the core checkout:

```sh
cargo run --locked --features duckdb --example managed_graph
```

| Core feature | Purpose |
| --- | --- |
| default (empty) | SQL compiler without database drivers |
| `duckdb` | Managed engine APIs, bundled executor, and legacy managed CLI |
| `postgres` | Lower-level PostgreSQL executor; not a federated client |

The standalone CLI instructions above refer to `OrchidDB-cli`, not the legacy core binary.
