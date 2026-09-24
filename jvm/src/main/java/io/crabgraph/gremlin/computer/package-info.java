/**
 * Native-provider GraphComputer execution using Apache TinkerPop 3.7.4 vertex
 * programs. This is a distinct JVM GraphComputer execution profile, not Rust
 * traversal-planner execution. No reference graph implementation is used.
 *
 * <p>The initial implementation executes one worker, with bulk-synchronous
 * message delivery, broadcast/reduced memory, cloned worker programs, and
 * sorted map/reduce output. Arbitrary computation-local values remain in the
 * isolated view. Persisted computation values use the provider's typed storage
 * and runtime-value contract. ResultGraph.NEW comes from a native graph factory;
 * its lifetime is owned by the caller/provider session family.</p>
 */
package io.crabgraph.gremlin.computer;
