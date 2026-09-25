#!/usr/bin/env bash
# Build and test locally against the pinned, unmodified Apache gremlin-test suite.
set -euo pipefail
if [[ "${GITHUB_ACTIONS:-false}" == "true" ]]; then
  echo "Conformance runs locally. GitHub Actions only publishes recorded results." >&2
  exit 1
fi
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$repo_root/target}"
export CARGO_PROFILE_DEV_DEBUG="${CARGO_PROFILE_DEV_DEBUG:-0}"
export CONFORMANCE_JAVA="${CONFORMANCE_JAVA:-${JAVA_HOME:+$JAVA_HOME/bin/}java}"
export CRABGRAPH_JAVA="$CONFORMANCE_JAVA"
export CONFORMANCE_TINKERPOP_SOURCE="${CONFORMANCE_TINKERPOP_SOURCE:-${CONFORMANCE_UPSTREAM_CACHE:-$repo_root/conformance/upstream/cache}/tinkerpop}"
result_path="${1:-$repo_root/target/conformance-datafusion/crabgraph-tinkerpop.json}"

cargo build --bin crabgraph-jvm-store
export CRABGRAPH_JVM_STORE="$(cd "$CARGO_TARGET_DIR" && pwd)/debug/crabgraph-jvm-store"
mvn -q -f jvm/pom.xml install -DskipTests dependency:build-classpath -Dmdep.outputFile=target/classpath.txt
mvn -q -f jvm-codecs/pom.xml install -DskipTests
mvn -q -f conformance/adapters/sqlg/pom.xml package -DskipTests dependency:build-classpath -Dmdep.outputFile=target/classpath.txt
export CRABGRAPH_JVM_CLASSPATH="$repo_root/jvm/target/classes:$(cat jvm/target/classpath.txt)"
export CONFORMANCE_GREMLIN_CLASSPATH="$repo_root/conformance/adapters/sqlg/target/classes:$(cat conformance/adapters/sqlg/target/classpath.txt)"
export CRABGRAPH_GREMLIN_IO_JAVA="$CONFORMANCE_JAVA"
export CRABGRAPH_GREMLIN_IO_CLASSPATH="$CONFORMANCE_GREMLIN_CLASSPATH"

mvn -q -f jvm/pom.xml test
cargo test --lib scheduling_tests
cargo test --test engine --test engine_adversarial --test engine_incremental \
  --test engine_storage_integrity --test engine_functions \
  --test gremlin_group_finalization --test gremlin_shared_side_effects \
  --test gremlin_upstream_regressions --test relational_execution
cargo test --test jvm_ir -- --include-ignored --test-threads=1
cargo build --manifest-path conformance/runner/Cargo.toml --bin upstream
export CONFORMANCE_CRABGRAPH_BINARY="$(cd "$CARGO_TARGET_DIR" && pwd)/debug/upstream"
python3 conformance/upstream/run.py --engine crabgraph --suite tinkerpop --output "$result_path"
