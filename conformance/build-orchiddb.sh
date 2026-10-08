#!/usr/bin/env bash
# Build conformance executables without starting any tests or suites.
set -euo pipefail
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"
export DUCKDB_DOWNLOAD_LIB="${DUCKDB_DOWNLOAD_LIB:-1}"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$repo_root/target}"
build_profile="${CONFORMANCE_CARGO_PROFILE:-dev}"
cargo build --profile "$build_profile" --manifest-path conformance/runner/Cargo.toml --bin upstream --bin orchiddb-jvm-store
output_profile="$build_profile"
if [[ "$output_profile" == dev ]]; then output_profile=debug; fi
printf 'Conformance binaries: %s/%s/{upstream,orchiddb-jvm-store}\n' "$CARGO_TARGET_DIR" "$output_profile"
