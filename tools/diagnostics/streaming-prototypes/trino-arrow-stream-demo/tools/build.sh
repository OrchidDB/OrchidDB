#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
cargo build --release --locked --manifest-path server/Cargo.toml
mvn -q -f connector/pom.xml org.apache.maven.plugins:maven-dependency-plugin:3.8.1:copy-dependencies -DoutputDirectory=target/plugin
mkdir -p connector/target/classes
# Compile with the same JDK and SPI as the server, without installing another JDK.
docker run --rm --user "$(id -u):$(id -g)" --entrypoint sh \
  -v "$PWD/connector:/work" -w /work trinodb/trino:484 -c '
  javac -cp "/usr/lib/trino/lib/*:target/plugin/*" -d target/classes src/main/java/demo/*.java
  cp -R src/main/resources/. target/classes/
  jar --create --file target/plugin/arrow-demo.jar -C target/classes .
'
