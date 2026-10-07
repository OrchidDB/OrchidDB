# Runnable Java examples

Run from the repository root with Java 17+, Maven, and Rust:

```sh
clients/java/scripts/run-example.sh BringYourOwnDuckDb
clients/java/scripts/run-example.sh ArrowBatches
clients/java/scripts/run-example.sh ClientFunctions
clients/java/scripts/run-example.sh MultipleEngines
clients/java/scripts/run-gremlin-example.sh
```

The launchers build the local JNI runtime and Java API. Each example registers
schema and executes graph queries using an application-owned connection.
There is no standalone compiler example. RDF mappings belong in GraphMapping;
query text and parameters are passed separately through Query.

The Arrow launcher configures the JVM access flag needed by Arrow. See
[Arrow ownership](../docs/arrow.md) and [the client README](../README.md).
