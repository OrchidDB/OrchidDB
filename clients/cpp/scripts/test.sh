#!/usr/bin/env bash
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/.." && pwd)
DUCKDB_ROOT=${DUCKDB_ROOT:-"$ROOT/.test-deps/duckdb-1.5.2"}
case "$(uname -s)-$(uname -m)" in
  Darwin-*) ASSET=osx-universal; LIBRARY=libduckdb.dylib; SHA=524f3537330a1b747556a0c98b62a46865a3f48c7ead2b2035c62f1ad3e5ca8b ;;
  Linux-x86_64) ASSET=linux-amd64; LIBRARY=libduckdb.so; SHA=4711438f0fdb04f0441803409bec5430b763d4f2ac3482c1f97cfa6b5ecb4c15 ;;
  Linux-aarch64) ASSET=linux-arm64; LIBRARY=libduckdb.so; SHA=b4acbd9d871c99788c303af982f2e39325f28fd415f9a927d0a73e206f09e75d ;;
  *) echo 'Set up CMake and DuckDB manually on this platform; see README' >&2; exit 1 ;;
esac
if [[ ! -f "$DUCKDB_ROOT/$LIBRARY" || ! -f "$DUCKDB_ROOT/duckdb.h" ]]; then
  mkdir -p "$DUCKDB_ROOT"
  curl -fL "https://github.com/duckdb/duckdb/releases/download/v1.5.2/libduckdb-$ASSET.zip" -o "$DUCKDB_ROOT/duckdb.zip"
  if command -v sha256sum >/dev/null; then
    ACTUAL=$(sha256sum "$DUCKDB_ROOT/duckdb.zip" | cut -d ' ' -f 1)
  else
    ACTUAL=$(shasum -a 256 "$DUCKDB_ROOT/duckdb.zip" | cut -d ' ' -f 1)
  fi
  [[ "$ACTUAL" = "$SHA" ]] || { echo 'DuckDB archive checksum mismatch' >&2; exit 1; }
  unzip -qo "$DUCKDB_ROOT/duckdb.zip" -d "$DUCKDB_ROOT"
fi
cmake -S "$ROOT" -B "$ROOT/build" -DORCHIDDB_BUILD_TESTS=ON \
  -DDUCKDB_INCLUDE_DIR="$DUCKDB_ROOT" -DDUCKDB_LIBRARY="$DUCKDB_ROOT/$LIBRARY"
cmake --build "$ROOT/build"
ctest --test-dir "$ROOT/build" --output-on-failure
