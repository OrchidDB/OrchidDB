An embedded graph query engine written in Rust, built with DuckDB, DataFusion,
and Apache Arrow.

These archives contain the `orchiddb` command-line tool with bundled DuckDB.
The CLI supports Cypher and Gremlin. SPARQL is available through the Rust
library, not the CLI. This is an early release; consult the guides and recorded
conformance results for supported behavior and limitations.

Download the archive matching your platform, verify it against `SHA256SUMS`,
and extract it. Then run:

```sh
./orchiddb --help
./orchiddb --query 'RETURN 1 AS value'
```

On Windows, use `orchiddb.exe`. macOS builds are not Developer ID signed or
notarized. The JVM bridge and Java dependencies are not included.

Each archive includes the current license, usage instructions, and `BUILD.json`
with the source commit, target, Rust version, and executable checksum. Source
archives are available with this release for building the Rust library.

- [Documentation](https://docs.orchiddb.com/)
- [Quickstart](https://docs.orchiddb.com/quickstart.html)
- [Conformance and scope](https://docs.orchiddb.com/conformance.html)
- [License](https://github.com/OrchidDB/OrchidDB/blob/main/LICENSE.md)
