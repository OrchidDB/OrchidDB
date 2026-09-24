# Pinned upstream Java provider tests

This local harness runs the original TinkerPop 3.7.4 JUnit classes and assertions against production `io.crabgraph.gremlin.CrabGraph`, backed by `crabgraph-jvm-store`. Fixture loading uses the upstream provider loader and writes to that native graph. Standard traversal results establish **JVM provider execution**, not native Rust traversal execution. `ProcessComputerSuite` selects the separate **JVM GraphComputer** profile.

`placeholders.json` maps all 15 original unsupported Gherkin stubs to exact Java class/method names and pinned source hashes. The stubs remain skipped. The six original writer tests only check that output files have bytes; substantive roundtrip tests remain separately identified supplemental evidence.

Build production `jvm/` and `crabgraph-jvm-store`, then run locally:

```sh
export JAVA_HOME=/opt/homebrew/opt/openjdk@21
export PATH="$JAVA_HOME/bin:$PATH"
python3 conformance/adapters/jvm-provider-tests/run.py \
  --upstream /path/to/pinned/tinkerpop \
  --store /path/to/crabgraph-jvm-store \
  --output /tmp/crabgraph-java-placeholders.json
```

Use `--selection ProcessStandardSuite`, `StructureStandardSuite`, or `ProcessComputerSuite` for the exact class list declared in that pinned upstream suite. Individual `fully.qualified.Class$Traversals#method` selections are supported. The runner invokes the original class runners, including parameterization; it does not claim the suite's separate provider-registration enforcement checks. `--inventory` enumerates descriptions/annotations without opening a graph. Baseline inventories are 937 standard process tests, 868 computer process tests, and 943 structure tests; these are **not test passes**.

JUnit assumption failures and ignored cases remain `skipped`, including feature exclusions. Reports include original annotations, source hashes, actual errors, production source/binary fingerprints and an incomplete-run marker on timeout. A complete run with failures exits 1; an overall timeout exits 124. Results belong in separately named artifacts and must not be folded into the 1,511-case Gherkin denominator. No CI execution is configured.
