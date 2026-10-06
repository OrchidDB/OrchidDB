# Extension runtime

DuckDB schedules compiled relational operations and Orchid's existing graph kernels
in the calling connection. The shared runtime compiler produces immutable kernel
programs; the C++ extension binds their SQL regions and supplies a typed Arrow
boundary. Apply, branch, repeat, merge, group, and mutation kernels use the same
subplan runner and statement state.

Multi-input kernel sources execute children in declared order before passing tagged
inputs to the existing kernel. This preserves side effects and keeps mutable host
state on its owning execution thread. Cancellation uses a separately owned atomic
token; its watcher joins before query state is destroyed.

Nested subplans reuse prepared DuckDB plans within a query. Each invocation attaches
its current kernel context temporarily. Query-end cleanup removes cached plans and
borrowed pointers. PREPARE and EXPLAIN compile without executing writes.

Mapped and managed reads/writes go through `src/ir/rel/host`, extracted from the
existing storage implementation. Graph values reuse native codecs. Existing JVM
algorithms and callbacks remain kernel operations rather than whole-query delegation.

The extension never invokes the retained DataFusion execution adapter or GraphEngine
as a fallback. See [architecture](architecture.md) and [verification](verification.md).
