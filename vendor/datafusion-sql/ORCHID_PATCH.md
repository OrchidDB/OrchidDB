# OrchidDB packaging patch

Source: Apache DataFusion `datafusion-sql` 53.1.0, crates.io release
https://crates.io/crates/datafusion-sql/53.1.0 (Apache-2.0).
`LICENSE.txt` and `NOTICE.txt` are retained from the release.

Only the normalized Cargo manifest is changed: its normal and matching development dependencies on
`datafusion-functions-nested` uses the adjacent vendored package, pinned to
53.1.0. This keeps the compiler-only function dependency graph intact when
OrchidDB is a dependency of another workspace, where root `[patch.crates-io]`
settings are not inherited. SQL implementation, tests, and original manifest
are unchanged.

Published crate SHA-256: `fa0d133ddf8b9b3b872acac900157f783e7b879fe9a6bccf389abebbfac45ec1`.
