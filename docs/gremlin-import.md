# Gremlin file import

`g.io(path).read()` is a graph write. It returns no rows and appends the file's vertices and edges to the current graph. Managed `GraphEngine` queries use the same persistence and statement rollback path as other mutations. Imports inside an explicit transaction are visible to subsequent queries and are undone by rollback.

The `.json`/`.graphson`, `.xml`/`.graphml`, and `.kryo`/`.gryo` extensions select GraphSON, GraphML, and Gryo respectively. Use `.with(IO.reader, IO.graphson)`, `.with(IO.reader, IO.graphml)`, or `.with(IO.reader, IO.gryo)` to override the extension. Paths are local to the engine process. The conformance adapter resolves upstream data paths before sending a query.

The native GraphSON reader accepts the TinkerPop adjacency-list format (one JSON vertex object per record). It preserves declared numeric widths and validates the complete input before graph writes. Unknown typed values, duplicate IDs, missing endpoints, invalid JSON, and unsupported readers produce errors. Imported edge records are read from `outE`; the corresponding `inE` copies do not create duplicate edges.

GraphML and Gryo use the pinned TinkerPop 3.7.4 Java readers in `conformance/adapters/sqlg/src/main/java/ImportGraph.java`. The codec decodes a file into GraphSON; Rust validates and applies the resulting records through the graph transaction. It does not replace the engine graph or provide query results.

For standalone engine use, compile the codec with the adapter's Maven dependencies and set:

- `CRABGRAPH_GREMLIN_IO_JAVA`: the Java executable (defaults to `java`).
- `CRABGRAPH_GREMLIN_IO_CLASSPATH`: a Java classpath containing `ImportGraph` and TinkerPop's dependencies.

The local conformance Java bridge supplies both variables using its current JVM and classpath. Java codec failures are returned as import errors. The engine does not invoke a shell to run the codec.

File writing is not implemented; `io(path)` without `read()` is rejected.
