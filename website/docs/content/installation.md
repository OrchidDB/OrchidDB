Build the command-line tool and Rust library from source, then set up a local application.

## Prerequisites

Use a Rust toolchain with edition 2024 support, Cargo, Git, and a native C/C++ build toolchain. On macOS, install the Xcode command-line tools. On Linux, install your distribution's C/C++ compiler and development tools.

DuckDB is bundled with the default build. Allow time for its native compilation on the first build. The generated Cypher and Gremlin parsers are included in the repository.

## Build from source

```sh
git clone https://github.com/henneberger/new-graph.git
cd new-graph
cargo build --locked --release --bin crabgraph
./target/release/crabgraph --help
```

Install the binary into Cargo's executable directory:

```sh
cargo install --locked --path . --bin crabgraph
crabgraph --query 'RETURN 1 AS value'
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
new-graph = { path = "../new-graph" }
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
arrow = "58.2.0"
```

The package name is `new-graph`; the Rust import is `new_graph`. Use the [quickstart](/quickstart.html) as `src/main.rs`.

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

This example opens an in-memory graph, creates a relationship with a typed parameter, commits it, and prints the query results. Continue to the [quickstart](/quickstart.html) for a persistent CLI session and application example.
