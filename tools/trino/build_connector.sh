#!/bin/sh
set -eu
cd "$(dirname "$0")/../.."
repo=$PWD
mkdir -p target/trino-connector/classes target/trino-connector/plugin
mvn -q -f extensions/trino/pom.xml org.apache.maven.plugins:maven-dependency-plugin:3.8.1:copy-dependencies -DoutputDirectory="$repo/target/trino-connector/plugin"
docker run --rm --user "$(id -u):$(id -g)" --entrypoint sh \
  -v "$repo:/work" -w /work trinodb/trino:484 -c '
  javac -cp "/usr/lib/trino/lib/*:target/trino-connector/plugin/*" -d target/trino-connector/classes extensions/trino/src/main/java/io/orchiddb/trino/*.java
  cp -R extensions/trino/src/main/resources/. target/trino-connector/classes/
  jar --create --file target/trino-connector/plugin/orchid-trino.jar -C target/trino-connector/classes .
'
