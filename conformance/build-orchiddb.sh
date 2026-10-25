#!/usr/bin/env bash
# Build conformance executables without starting any tests or suites.
set -euo pipefail
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"
if [[ -n "${DUCKDB_LIB_DIR:-}" ]]; then
    export DUCKDB_DOWNLOAD_LIB="${DUCKDB_DOWNLOAD_LIB:-0}"
else
    export DUCKDB_DOWNLOAD_LIB="${DUCKDB_DOWNLOAD_LIB:-1}"
fi
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$repo_root/target}"
build_profile="${CONFORMANCE_CARGO_PROFILE:-dev}"
download_features=()
case "$DUCKDB_DOWNLOAD_LIB" in
    1|[Tt][Rr][Uu][Ee]) download_features=(--features orchiddb/duckdb-download) ;;
esac
cargo build --profile "$build_profile" --manifest-path conformance/runner/Cargo.toml "${download_features[@]}" --bin upstream --bin orchiddb-jvm-store
output_profile="$build_profile"
if [[ "$output_profile" == dev ]]; then output_profile=debug; fi
printf 'Conformance binaries: %s/%s/{upstream,orchiddb-jvm-store}\n' "$CARGO_TARGET_DIR" "$output_profile"
