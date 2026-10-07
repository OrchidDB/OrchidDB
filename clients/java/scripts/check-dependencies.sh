#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
mkdir -p target
cargo tree --manifest-path native/Cargo.toml --locked --prefix none > target/native-dependencies.txt
if grep -Eq '^(duckdb|libduckdb-sys) ' target/native-dependencies.txt; then
  echo 'Compiler unexpectedly depends on DuckDB' >&2
  exit 1
fi
mvn -q -pl orchiddb-java dependency:build-classpath -DincludeScope=runtime -Dmdep.outputFile="$PWD/target/runtime-classpath.txt"
if grep -Eqi 'duckdb_jdbc-[^:;]*\.jar' target/runtime-classpath.txt; then
  echo 'Java runtime unexpectedly depends on DuckDB' >&2
  exit 1
fi
echo 'No DuckDB dependency in the Rust compiler or Java runtime.'
