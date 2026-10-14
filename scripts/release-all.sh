#!/usr/bin/env bash
# Build every client locally; publication is opt-in.
set -euo pipefail
ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
usage() {
  cat <<'EOF'
Usage: scripts/release-all.sh [--version VERSION] [--publish | --check]

Run on an Apple Silicon Mac. Builds Linux AMD64, Linux ARM64, and macOS ARM64
clients, CLI, and DuckDB extension. Defaults to the version in Cargo.toml.
--publish  Also push main and publish all assets to GitHub (requires gh auth).
--check    Prepare the Python packaging environment and check prerequisites only.
--version  Prepare and commit a version bump from an otherwise clean checkout.

Requires Python 3.11+, Rust/rustup, Zig, cargo-zigbuild, Java 21, Maven,
Node 20+/npm, Go 1.23+, CMake, and Xcode command line tools.
Commit your changes before running. Completed builds are reused on retry.
RELEASE_JOBS controls concurrent platforms (default 3); CARGO_BUILD_JOBS
controls compiler jobs per platform (default 3).
EOF
}
mode=build
version=${VERSION:-}
while [[ $# -gt 0 ]]; do
  case "$1" in
    --help|-h) usage; exit 0 ;;
    --version)
      [[ $# -ge 2 && -n "$2" ]] || { echo '--version requires a value' >&2; exit 2; }
      version=$2; shift 2 ;;
    --publish|--check)
      [[ "$mode" == build ]] || { echo 'Choose only one of --publish or --check' >&2; exit 2; }
      mode=${1#--}; shift ;;
    *) echo "Unknown argument: $1" >&2; usage >&2; exit 2 ;;
  esac
done
cd "$ROOT"
PYTHON=${PYTHON:-python3}
"$PYTHON" -c 'import sys; sys.exit("Python 3.11+ is required" if sys.version_info < (3, 11) else 0)'
[[ "$(uname -s)" == Darwin && "$(uname -m)" == arm64 ]] || {
  echo 'Run this script on an Apple Silicon Mac (macOS ARM64).' >&2; exit 1;
}
if [[ -z "$version" ]]; then
  version=$("$PYTHON" -c 'import tomllib; print(tomllib.load(open("Cargo.toml", "rb"))["package"]["version"])')
fi
export VERSION="$version"
export RELEASE_PYTHON=${RELEASE_PYTHON:-"$ROOT/target/release-env/bin/python"}
export RELEASE_JOBS=${RELEASE_JOBS:-3}
export CARGO_BUILD_JOBS=${CARGO_BUILD_JOBS:-3}
if [[ "$mode" == publish ]]; then
  command -v gh >/dev/null || { echo 'Install gh and run gh auth login before publishing.' >&2; exit 1; }
  gh auth status
fi
if [[ "$mode" != check ]]; then
  make release-prepare "PYTHON=$PYTHON" "VERSION=$version"
fi
make release-env "PYTHON=$PYTHON"
if [[ "$mode" == check ]]; then
  exec "$PYTHON" scripts/release/release.py check --version "$version"
elif [[ "$mode" == publish ]]; then
  exec "$PYTHON" scripts/release/release.py all --version "$version"
fi
"$PYTHON" scripts/release/release.py build --version "$version"
"$PYTHON" scripts/release/release.py package --version "$version"
printf '\nRelease artifacts: %s/target/releases/orchiddb-v%s/%s/upload/\n' "$ROOT" "$version" "$(git rev-parse HEAD)"
