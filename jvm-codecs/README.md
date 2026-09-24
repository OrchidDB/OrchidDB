# Native graph file codecs

This optional production module converts native graph snapshots to/from pinned TinkerPop 3.7.4 GraphSON, Gryo and GraphML. It is separate from the production JVM traversal provider. TinkerGraph is used exclusively as a format-conversion buffer loaded from the actual input file/native snapshot. No fixture, traversal evaluation, expected result, or reference answer is used by these codecs.

Build locally with Java 21:

```sh
mvn -f jvm-codecs/pom.xml install dependency:build-classpath -Dmdep.outputFile=target/classpath.txt
export CRABGRAPH_GREMLIN_IO_JAVA=/path/to/java
export CRABGRAPH_GREMLIN_IO_CLASSPATH="/path/to/jvm-codecs/target/classes:$(cat jvm-codecs/target/classpath.txt)"
```

Native `g.io(path).write()` creates a consistent native snapshot and atomically replaces the destination only after conversion succeeds. Its graph state is unchanged by exports. GraphSON preserves supported native numeric widths, exact BigInteger/BigDecimal values, public IDs, list/set/map values, vertex property cardinality and meta-properties. Gryo uses the pinned upstream serializer. GraphML supports single primitive properties, stringifies IDs and cannot preserve vertex property IDs, multi-properties or metadata; unsupported native exports fail explicitly rather than silently flattening data.

`ImportGraph` reads an input graph using the pinned reader and emits typed GraphSON adjacency records. It explicitly emits null values without passing property objects through the upstream GraphSON serializer, whose null edge/meta-property path is unsupported in 3.7.4. Each non-null value is still serialized by the upstream typed mapper. `ExportGraph` consumes the real native adjacency snapshot and writes the selected binary/XML format. Native GraphSON output is written directly by Rust.

The legacy conformance entry points delegate to these production classes. The module has no CI test workflow; roundtrip and error tests run locally.
