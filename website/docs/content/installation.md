# Installation

Use the CLI for DuckDB and Iceberg, or embed a language client around your own engine. Version 0.1.0 is published. Packaged native binaries currently target macOS ARM64; source builds are also available.

## Binary installer

Install the released CLI with bundled DuckDB. No Rust toolchain or separate database installation is needed:

```sh
curl -fsSL https://install.orchiddb.com | bash
export PATH="$HOME/.local/bin:$PATH"
orchiddb --version
```

The installer downloads from [OrchidDB-cli releases](https://github.com/OrchidDB/OrchidDB-cli/releases), verifies SHA-256 checksums, and installs the executable and its bundled DuckDB library into `~/.local/bin`. It never runs sudo or edits your shell configuration. The `export` above updates the current shell; add it to your shell configuration to keep the command available in new terminals.

Version 0.1.0 provides a **macOS ARM64 (Apple Silicon)** binary. Other platforms require a source build. The macOS binary is ad-hoc signed, not notarized.

After installation, run `~/.local/bin/orchiddb --version`. Add `~/.local/bin` to your `PATH` to use `orchiddb` directly. [Direct download](https://github.com/OrchidDB/OrchidDB-cli/releases/download/v0.1.0/orchiddb-v0.1.0-macos-aarch64.tar.gz).

```sh
curl -fsSL https://install.orchiddb.com | ORCHIDDB_VERSION=v0.1.0 ORCHIDDB_INSTALL_DIR="$HOME/.local/bin" bash
```

Without a version, the installer selects the most recently created published release, including prereleases. [Review the installer](https://install.orchiddb.com/) before running it. Checksums come from the same release publisher as the archive.

Run the installer again to upgrade, or set `ORCHIDDB_VERSION` to select a specific release. Keep the installed `lib/libduckdb.dylib` alongside the executable on macOS. Continue with the [quickstart](quickstart.md) to run a query without cloning a repository.

## Published packages

The following 0.1.0 releases are available. The CLI includes DuckDB 1.5.2; embedded clients use an application-owned database driver.

| Component | Published distribution | Installation |
| --- | --- | --- |
| CLI | [GitHub release](https://github.com/OrchidDB/OrchidDB-cli/releases/tag/v0.1.0) | Installer above; macOS ARM64 binary |
| Rust | [orchiddb-client on crates.io](https://crates.io/crates/orchiddb-client/0.1.0) | `cargo add orchiddb-client@0.1.0 --no-default-features` |
| Python | [orchiddb on PyPI](https://pypi.org/project/orchiddb/0.1.0/) | `python -m pip install "orchiddb[arrow]==0.1.0"` |
| JavaScript / TypeScript | [@orchiddb/client on npm](https://www.npmjs.com/package/@orchiddb/client/v/0.1.0) | `npm install @orchiddb/client@0.1.0 apache-arrow@17` |
| Java | [com.orchiddb:orchiddb-java on Maven Central](https://central.sonatype.com/artifact/com.orchiddb/orchiddb-java/0.1.0) | [API and native compiler dependencies](client-apis.md#java) |
| Elixir | [orchiddb on Hex](https://hex.pm/packages/orchiddb/0.1.0) | `{:orchiddb, "~> 0.1.0"}` plus the [native compiler](client-apis.md#elixir) |
| C++ | [GitHub release](https://github.com/OrchidDB/OrchidDB-cpp/releases/tag/v0.1.0) | [Download and configure CMake](client-apis.md#c) |

Packaged native compilers in this release target macOS ARM64. The Python wheel requires macOS 26 or newer and Python 3.10+; the Node.js client requires Node.js 20+; Java requires Java 17+ and an ARM64 JVM. Rust compiles from source through Cargo. See [client setup](client-apis.md) for driver dependencies, native library setup, and runnable examples.

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

## Language clients

| Client | Source | Result interface |
| --- | --- | --- |
| Rust | [OrchidDB-rust](https://github.com/OrchidDB/OrchidDB-rust) | Arrow `RecordBatchReader` |
| Python | [OrchidDB-python](https://github.com/OrchidDB/OrchidDB-python) | PyArrow reader |
| JavaScript / TypeScript | [OrchidDB-js](https://github.com/OrchidDB/OrchidDB-js) | Async Arrow batches |
| Java | [OrchidDB-java](https://github.com/OrchidDB/OrchidDB-java) | Arrow vectors / `ArrowResult` |
| Elixir | [OrchidDB-elixir](https://github.com/OrchidDB/OrchidDB-elixir) | ADBC Arrow C Stream callback |
| C++ | [OrchidDB-cpp](https://github.com/OrchidDB/OrchidDB-cpp) | Arrow C stream with RAII ownership |

Start with [client setup and runnable examples](client-apis.md). These clients compile SQL without a DuckDB driver dependency. Your application supplies the engine. For Java, add the API and macOS ARM64 compiler JAR dependencies from Maven Central, then call `NativeSqlCompiler.load()`. Maven supplies the compiler automatically: no manual native binary download or Rust build is required. See [Java installation](client-apis.md#java) for the complete dependency block and Arrow setup. The Java 0.1.0 compiler package supports macOS ARM64 JVMs only. The shared compiler instructions below apply to source builds of the other bindings.

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

Install the published core compiler directly from crates.io:

```toml
[dependencies]
orchiddb = { version = "=0.1.0", default-features = false }
```

See [SQL compilation](sql-compiler.md). For application integration, prefer the [Rust client](client-apis.md#rust).

<a id="use-the-rust-library"></a>

## Rust GraphEngine

The optional `GraphEngine` runtime supports managed storage and mapped application
tables through one engine, including RDF reads and updates. The API tutorials describe the current core checkout. New RDF catalog,
composite-key, physical-layout, and collection-source APIs require a matching
source build; the published 0.1.0 binaries may bundle an earlier core revision. Their dependencies use an explicit `duckdb` feature:

```toml
[dependencies]
orchiddb = { path = "../orchiddb", features = ["duckdb"] }
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
arrow = "58.2.0"
duckdb = { version = "1.10502.0", features = ["bundled"] }
```

Run its separate example from the core checkout:

```sh
cargo run --locked --features duckdb --example managed_graph
```

| Core feature | Purpose |
| --- | --- |
| default (empty) | SQL compiler without database drivers |
| `duckdb` | Unified managed/mapped runtime APIs, bundled executor, and legacy core CLI |
| `postgres` | Lower-level PostgreSQL executor; not a federated client |

The standalone CLI instructions above refer to `OrchidDB-cli`, not the legacy core binary.
