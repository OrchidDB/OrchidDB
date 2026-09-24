# Native-provider JVM GraphComputer

Crabgraph's single-process GraphComputer executes the pinned TinkerPop 3.7.4 vertex programs over `CrabGraph`, whose vertices, edges, properties, identity, and mutations belong to the native store. It is a separate execution profile from the native Rust traversal planner. It does not construct a TinkerGraph or substitute reference results.

The public entry points are `graph.compute()` and `graph.traversal().withComputer()`. See [JVM executor setup](gremlin-jvm-executor.md) for the production module and native-store executable. The conformance adapter identifies this profile as `crabgraph-computer`; native Rust evidence remains separately reported as `crabgraph`.

## Execution and state

One worker runs the upstream vertex program through supersteps, with iteration-delayed local/global messages, optional message combination, broadcast memory, memory reducers, worker lifecycle, termination, and map/reduce output sorting. `ComputerGraph` enforces the upstream star-graph access rules. Graph filters constrain loaded vertices, edges, and vertex properties.

A provider-backed view isolates compute properties from ordinary graph properties. It supports arbitrary JVM computation-local values, compute-key cardinality and metadata, and transient-key cleanup. Rich halted-traverser values that cross chained jobs use the native provider's explicit session-family runtime references; these are runtime state, not durable serialized graph properties.

A computation holds the provider's execution lease. Native source vertex, edge and meta-property records are cached only for that immutable view, and computed-key reads resolve directly from the overlay. `Persist.NOTHING` leaves the original graph unchanged. Publishing to `ResultGraph.ORIGINAL` uses a native savepoint; failures restore prior properties and preserve unrelated caller transaction writes. Caller-owned transactions remain open. A transaction opened by the computation is committed only after successful original-graph publication, or rolled back when it held only reads.

`ResultGraph.NEW` creates another native-backed graph through the provider factory. `VERTEX_PROPERTIES` copies filtered vertices with their existing/explicit property IDs, cardinality and metadata; `EDGES` also copies filtered edges and their properties. Copying reserves all existing and explicit property IDs before allocating IDs for generated compute properties, so configured native ID types and future source IDs are respected. Original-graph publication also reserves explicit IDs before allocating generated IDs. It uses an atomic native batch. Failure closes the incomplete new graph. A successfully returned new graph survives closing the original; callers close result graphs, or use `closeFamily()` when disposing an entire traversal session and its intermediate graphs.

Cancellation interrupts the worker. The provider drains a submitted native response before propagating cancellation, so the protocol remains synchronized. Cleanup temporarily clears the interrupt flag while rolling back owned read transactions or savepoints, then restores it. Explicit executor abort remains a separate operation that terminates its session family.

## Supported algorithms and boundaries

ConnectedComponent, PageRank, PeerPressure and ShortestPath use the pinned upstream vertex-program implementations, including their traversal modulators, edge selection, property names, iteration settings and chaining. The native Rust grammar path explicitly reports that these steps require the JVM GraphComputer profile. The previous fixture-name-based virtual algorithm properties have been removed; ordinary reads return only stored property values.

The initial computer supports one worker. It advertises no vertex/edge addition or removal, no edge-property mutation, and no vertex-property removal capability. Compute-key replacement and lifecycle cleanup remain supported. This is an in-process execution capability; distributed execution is not claimed.

## Verification profiles

All execution is local. Original assertions and pinned source hashes remain unchanged. The [algorithm Java selection](../conformance/adapters/jvm-provider-tests/computer-algorithms.json) identifies the 31 original methods. The [provider harness](../conformance/adapters/jvm-provider-tests/README.md) records original assumptions and the official `is.testing` and `assertNonDeterministic` flags separately from outcomes.

The upstream ProcessComputerSuite declares 868 test occurrences and 852 unique method IDs: its 16 ProfileTest methods appear twice. Reports must retain both counts and execute both declared occurrences. Supplemental tests are reported separately from the upstream suites.

The native algorithm-boundary change preserves the baseline passing set: 1,427 passes, 83 skips, one unsupported case, and zero failures/timeouts/adapter errors in the unchanged 1,511-case catalog. Twenty-one supplemental native-provider computer tests cover arbitrary topology and renamed IDs, weighted tied paths, direction and distance limits, zero iterations and algorithm chaining, result/persist modes, filters, multi/meta-properties, messages/memory/transients and mutable broadcast reducer isolation, result ownership, publication rollback and cancellation. Final upstream computer evidence is recorded separately from this native baseline and supplemental test count.
