# Shared native SDK runtime

This library is the private execution-planning and transport implementation used
by OrchidDB's foreign-language SDKs. Applications use each SDK's Connection API:
register schema once, then execute text and parameters separately.

There is no customer-facing JSON compiler protocol or compile-to-SQL command.
The SDK transport still serializes internal execution work across language
boundaries and shares the same core with Rust and the optional DuckDB extension.

From the repository root, run `make native`. It stages the runtime for Python,
Node, and C++; Elixir can use `ORCHIDDB_NATIVE_LIBRARY`. Java uses its JNI binding.
This interface is **ABI 2**: rebuild clients and runtime together. The obsolete
`orchiddb_compile_json` symbol has been removed. The SDK-only execution command,
Arrow binding, statistics, remote transport, and string ownership declarations
are in [orchiddb.h](include/orchiddb.h).

Calls use bounded native worker stacks. Returned strings must be released with
`orchiddb_string_free`; version and revision strings are borrowed. Arrow binding
consumes its stream. Applications retain their database connections and results.

Source identity is generated from this checkout. Release packaging requires a
clean identified revision and matching metadata/checksums. Never replace
published artifacts or tags. This source change publishes no binary package.

[Client build instructions](../README.md)
