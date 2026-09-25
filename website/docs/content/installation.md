# Installation

Choose the command-line client or a language library.

## Command line installer

```sh
curl -fsSL https://install.orchiddb.com | bash
```

The installer downloads a native CLI archive from [published GitHub releases](https://github.com/OrchidDB/OrchidDB/releases), verifies its SHA-256 checksum, and installs into `~/.local/bin`. It never runs sudo or edits your shell configuration. Add that directory to your `PATH` if necessary.

**There are no published binary releases yet.** Until the first release is published, use the [source installation](#from-source). The installer explains this and leaves any existing installation unchanged.

Supported installer platforms: macOS Apple Silicon, macOS Intel, and Linux x86_64 with glibc 2.35+. Windows x86_64 uses the ZIP asset on the release page; extract it and run `orchiddb.exe`. Linux ARM64 and musl builds are not packaged. macOS binaries are not signed or notarized.

To select an exact release and directory:

```sh
curl -fsSL https://install.orchiddb.com | ORCHIDDB_VERSION=v0.1.0 ORCHIDDB_INSTALL_DIR="$HOME/.local/bin" bash
```

Without a version, the installer selects the most recently created published release, including prereleases. Review the [installer source](https://github.com/OrchidDB/OrchidDB/blob/main/website/install/install.sh) or download it before running it. Checksums detect corrupted downloads; they are provided by the same release publisher as the archive.

## Language clients

Rust is available as a Git or path dependency below. Python, JavaScript/TypeScript, and Java have **mock download assets** for the installation interface. These ZIPs contain metadata and documentation links only; they are not installable SDKs.

- [Python placeholder](https://install.orchiddb.com/mock/orchiddb-python-placeholder.zip)
- [JavaScript / TypeScript placeholder](https://install.orchiddb.com/mock/orchiddb-javascript-placeholder.zip)
- [Java placeholder](https://install.orchiddb.com/mock/orchiddb-java-placeholder.zip)

The release workflow also includes these explicitly named placeholder assets in each release. See the [client API design](client-apis.md) for integration details.


## Prerequisites

Use a Rust toolchain with edition 2024 support, Cargo, Git, and a native C/C++ build toolchain. On macOS, install the Xcode command-line tools. On Linux, install your distribution's C/C++ compiler and development tools.

DuckDB is bundled with the default build. Allow time for its native compilation on the first build. The generated Cypher and Gremlin parsers are included in the repository.

## From source

```sh
git clone https://github.com/OrchidDB/OrchidDB.git orchiddb
cd orchiddb
cargo build --locked --release --bin orchiddb
./target/release/orchiddb --help
```

Install the binary into Cargo's executable directory:

```sh
cargo install --locked --path . --bin orchiddb
orchiddb --query 'RETURN 1 AS value'
```

Ensure Cargo's executable directory, usually `$HOME/.cargo/bin`, is on your shell's `PATH`.

## Use the Rust library

Create an application beside the repository:

```sh
cargo new graph-app
```

Add these dependencies to `graph-app/Cargo.toml`. Adjust the path to your checkout.

```toml
[dependencies]
orchiddb = { path = "../orchiddb" }
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
arrow = "58.2.0"
```

The package name is `orchiddb`; the Rust import is `orchiddb`. Use the [quickstart](quickstart.md) as `src/main.rs`.

## Cargo features

| Feature | Purpose |
| --- | --- |
| `duckdb` | Default feature. Includes the bundled DuckDB executor, engine APIs, and CLI. |
| `postgres` | Includes the PostgreSQL SQL executor for lower-level SQL integration. |

To build the core library without default features:

```sh
cargo check --locked --no-default-features --lib
```

To include the PostgreSQL executor alongside the defaults:

```sh
cargo build --locked --features postgres
```

## Run the repository example

```sh
cargo run --locked --example managed_graph
```

This example opens an in-memory graph, creates a relationship with a typed parameter, commits it, and prints the query results. Continue to the [quickstart](quickstart.md) for a persistent CLI session and application example.
