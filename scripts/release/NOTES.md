An embedded graph query engine written in Rust, built with DuckDB, DataFusion,
and Apache Arrow.

These archives contain the `crabgraph` command-line tool with bundled DuckDB.
The CLI supports Cypher and Gremlin. SPARQL is available through the Rust
library, not the CLI. This is an early release; consult the guides and recorded
conformance results for supported behavior and limitations.

Download the archive matching your platform, verify it against `SHA256SUMS`,
and extract it. Then run:

```sh
./crabgraph --help
./crabgraph --query 'RETURN 1 AS value'
```

On Windows, use `crabgraph.exe`. macOS builds are not Developer ID signed or
notarized. The JVM bridge and Java dependencies are not included.

Each archive includes the current license, usage instructions, and `BUILD.json`
with the source commit, target, Rust version, and executable checksum. Source
archives are available with this release for building the Rust library.

- [Documentation](https://docs.crabgraph.net/)
- [Quickstart](https://docs.crabgraph.net/quickstart.html)
- [Conformance and scope](https://docs.crabgraph.net/conformance.html)
- [License](https://github.com/henneberger/new-graph/blob/main/LICENSE.md)
