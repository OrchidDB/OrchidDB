#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
python3 ../../scripts/clients.py metadata
cargo build --manifest-path native/Cargo.toml --locked
case "$(uname -s)" in
  Darwin) lib=liborchiddb_java.dylib ;;
  Linux) lib=liborchiddb_java.so ;;
  *) echo 'Use Maven with -Dorchiddb.native.path pointing to your compiled JNI library.' >&2; exit 1 ;;
esac
native_dir="${CARGO_TARGET_DIR:-$PWD/../../target}"
native_dir="$(cd "$native_dir" && pwd)"
mvn -Dorchiddb.native.path="$native_dir/debug/$lib" "${ORCHIDDB_MAVEN_GOAL:-verify}" "$@"
