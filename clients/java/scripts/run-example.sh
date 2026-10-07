#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
example="${1:-BringYourOwnDuckDb}"
case "$example" in BringYourOwnDuckDb|ClientFunctions|OfflineSql|MultipleEngines|ArrowBatches|GremlinExample) ;; *) echo 'Unknown example' >&2; exit 1;; esac
profile=""
main_class="io.orchiddb.examples.$example"
if [[ "$example" == GremlinExample ]]; then
  profile=-Pgremlin
  main_class=io.orchiddb.gremlin.GremlinExample
fi
# Build and install the API from this checkout before resolving the example.
ORCHIDDB_MAVEN_GOAL=install ./scripts/build.sh -DskipTests ${profile:+"$profile"}
case "$(uname -s)" in
  Darwin) library=liborchiddb_java.dylib ;;
  Linux) library=liborchiddb_java.so ;;
  *) echo "Unsupported source build platform" >&2; exit 1 ;;
esac
native_dir="${CARGO_TARGET_DIR:-$PWD/../../target}"
mvn -q ${profile:+"$profile"} -f examples/pom.xml compile dependency:build-classpath -Dmdep.outputFile=target/classpath.txt
java --add-opens=java.base/java.nio=ALL-UNNAMED -Dorchiddb.native.path="$native_dir/debug/$library" \
  -cp "examples/target/classes:$(cat examples/target/classpath.txt)" "$main_class"
