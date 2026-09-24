# Gremlin file import

`g.io(path).read()` is a graph write. It returns no rows and appends the file's vertices and edges to the current graph. Managed `GraphEngine` queries use the same persistence and statement rollback path as other mutations. Imports inside an explicit transaction are visible to subsequent queries and are undone by rollback.

The `.json`/`.graphson`, `.xml`/`.graphml`, and `.kryo`/`.gryo` extensions select GraphSON, GraphML, and Gryo respectively. Use `.with(IO.reader, IO.graphson)`, `.with(IO.reader, IO.graphml)`, or `.with(IO.reader, IO.gryo)` to override the extension. Paths are local to the engine process. The conformance adapter resolves upstream data paths before sending a query.

The native GraphSON reader accepts the TinkerPop adjacency-list format (one JSON vertex object per record). It preserves declared numeric widths and validates the complete input before graph writes. Unknown typed values, duplicate IDs, missing endpoints, invalid JSON, and unsupported readers produce errors. Imported edge records are read from `outE`; the corresponding `inE` copies do not create duplicate edges.

GraphML and Gryo use the pinned TinkerPop 3.7.4 Java readers in `conformance/adapters/sqlg/src/main/java/ImportGraph.java`. The codec decodes a file into GraphSON; Rust validates and applies the resulting records through the graph transaction. It does not replace the engine graph or provide query results.

For standalone engine use, compile the codec with the adapter's Maven dependencies and set:

- `CRABGRAPH_GREMLIN_IO_JAVA`: the Java executable (defaults to `java`).
- `CRABGRAPH_GREMLIN_IO_CLASSPATH`: a Java classpath containing `ImportGraph` and TinkerPop's dependencies.

The local conformance Java bridge supplies both variables using its current JVM and classpath. Java codec failures are returned as import errors. The engine does not invoke a shell to run the codec.

`io(path)` requires exactly one of `read()` or `write()`.

## File export

`g.io(path).write()` reads a private native graph snapshot and returns no rows. The same extensions select the writer; `.with(IO.writer, IO.graphson)`, `.with(IO.writer, IO.graphml)`, or `.with(IO.writer, IO.gryo)` overrides selection. Options may follow `write()`. Export is allowed by `ReadOnlyStrategy` because it does not mutate the graph.

GraphSON is written natively in the TinkerPop 3.7.4 GraphSON 3 adjacency format. It preserves public vertex/edge/property IDs, labels, both edge directions, property cardinality, meta-properties, numeric widths, arbitrary precision numbers, lists, sets and typed maps. Unsupported native value kinds produce an error instead of being converted to strings. A valid empty graph has an empty GraphSON file.

Gryo and GraphML use `ExportGraph` on the same codec classpath as `ImportGraph`. The Java codec reads the actual native GraphSON snapshot solely to convert its file representation. It does not execute traversals or supply graph query results. GraphML requires single boolean/string/Int32/Int64/Float/Double properties, one scalar type per key, and rejects nulls, collections, multi-properties and meta-properties. GraphML IDs become strings and vertex-property IDs are not represented. Choose GraphSON or Gryo when those identities/types must survive.

The writer stages files in the destination directory, flushes completed output, then atomically renames the output over the destination. Serialization, codec and rename failures leave an existing destination intact and remove staging files. The parent directory must exist. Exported files reflect graph state visible at invocation, including uncommitted graph changes; subsequent transaction rollback does not remove an already exported file. Rename completion is the file visibility contract; this does not promise directory-entry durability across a machine crash.

Supplemental Rust tests are in `tests/gremlin_export.rs`. The ignored codec test must be run explicitly with the pinned classpath configured; it checks all six default/explicit writer choices using the independent upstream reader and the native importer. These executable round trips are separate from upstream Gherkin writer placeholders.

Null values in GraphSON and Gryo retain presence when importing into a graph configured with `enable_null_property_values(true)`. The codec emits its adjacency envelope directly with the pinned typed mapper because the upstream 3.7.4 GraphSON graph writer cannot serialize null edge/meta-property values.
