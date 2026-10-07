# Runnable Java examples

For use in your own application, follow [Install from Maven Central](../README.md#install-from-maven-central) and call `NativeSqlCompiler.load()`. Maven supplies the packaged compiler automatically. The launchers use the standalone `examples/pom.xml` to resolve version 0.2.1 dependencies. Keep API and native compiler versions aligned. The compiler package supports macOS ARM64 JVMs.

Run these commands from `~/orchiddb/orchiddb-java` with Java 17 or newer. On macOS, set `export JAVA_HOME=$(/usr/libexec/java_home -v 17)` first.

| Example | Source | Run |
| --- | --- | --- |
| Arrow batches over your connection | [ArrowBatches.java](../orchiddb-java/src/test/java/io/orchiddb/examples/ArrowBatches.java) | `./scripts/run-example.sh ArrowBatches` |
| Native Java Gremlin traversals | [GremlinExample.java](../orchiddb-gremlin/src/test/java/io/orchiddb/gremlin/GremlinExample.java) | `./scripts/run-gremlin-example.sh` |
| Cypher on your existing DuckDB connection | [BringYourOwnDuckDb.java](../orchiddb-java/src/test/java/io/orchiddb/examples/BringYourOwnDuckDb.java) | `./scripts/run-example.sh BringYourOwnDuckDb` |
| Caller-registered Java UDF | [ClientFunctions.java](../orchiddb-java/src/test/java/io/orchiddb/examples/ClientFunctions.java) | `./scripts/run-example.sh ClientFunctions` |
| SQL compilation without executing a database | [OfflineSql.java](../orchiddb-java/src/test/java/io/orchiddb/examples/OfflineSql.java) | `./scripts/run-example.sh OfflineSql` |
| Routing to multiple engine instances | [MultipleEngines.java](../orchiddb-java/src/test/java/io/orchiddb/examples/MultipleEngines.java) | `./scripts/run-example.sh MultipleEngines` |

The launchers build the local JNI binding and install the Java API from this checkout before running. They select the resulting native library automatically. The Gremlin launcher includes the local `orchiddb-gremlin` module. Rust, Java 17+, and Maven are required.

Example source lives under Maven test sources for reuse by development builds; the standalone example POM compiles only example classes. Import `examples/pom.xml` into your IDE to run them against published packages.

The Arrow launcher adds the JVM `--add-opens` option. See [Arrow setup and ownership](../docs/arrow.md) when running from your own application or IDE.
