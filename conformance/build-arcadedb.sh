#!/usr/bin/env bash
# Build only. Suites run separately on the local workstation.
set -euo pipefail
cd "$(dirname "$0")/.."
mvn -q -f conformance/adapters/arcadedb/pom.xml package -DskipTests dependency:build-classpath -Dmdep.outputFile=target/classpath.txt
mvn -q -f conformance/adapters/sqlg/pom.xml -Parcadedb package -DskipTests dependency:build-classpath -Dmdep.outputFile=target-arcadedb/classpath.txt
mvn -q -f conformance/adapters/sqlg/pom.xml -Parcadedb dependency:copy -Dartifact=org.antlr:antlr4-runtime:4.9.1 -DoutputDirectory=target-arcadedb
mvn -q -f conformance/adapters/sqlg/pom.xml -Parcadedb dependency:copy -Dartifact=org.apache.tinkerpop:gremlin-core:3.7.4 -DoutputDirectory=target-arcadedb
