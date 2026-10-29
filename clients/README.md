# Library bindings

Rust (`clients/rust`) and Go (`clients/go`) use the shared Cypher core. The Go
binding loads the internal C ABI in `clients/native`. Applications own their
SQL sessions and consume Arrow results.

Run `make clients-check`, `make native`, and `make go-test` from the repository
root. See the [root README](../README.md) for setup and usage.
