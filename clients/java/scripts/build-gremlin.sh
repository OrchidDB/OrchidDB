#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
ORCHIDDB_MAVEN_GOAL=install ./scripts/build.sh -Pgremlin "$@"
