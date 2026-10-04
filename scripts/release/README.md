# Local coordinated releases

All compilation, runtime tests, cross-compilation, and packaging run locally.
GitHub is only the destination for finished release assets. Do not dispatch
GitHub Actions builds. This applies to every client and platform.

From the nine-repository workspace:

```sh
make release VERSION=X.Y.Z
```

The committed entry point is `scripts/release/Makefile`. The workspace Makefile
forwards to it. Individual stages are resumable:

```sh
make release-build VERSION=X.Y.Z
make release-package VERSION=X.Y.Z
make release-verify VERSION=X.Y.Z
make release-publish VERSION=X.Y.Z
```

Prepare consistent versions and dependency pins, run the local integration
checks, then commit and tag the tested source. Release state and test evidence
live in `.releases/X.Y.Z/`. Local build worktrees use the immutable tested
revisions. Successful binaries and packages are reused by source revision and
checksum; only missing or failed work is repeated.

On macOS, the local builder uses Xcode for both Mac architectures, cargo-zigbuild
for Linux. Cross-tooling lives
in `.releases/tooling`; persistent output and SDK caches live under `target/`.
The CLI links checksum-pinned official DuckDB static libraries. Its Linux C++
sysroot can be prepared locally with `scripts/release/local_linux_sdk.sh`.

Build the shared C ABI and JNI libraries for Linux x86_64, macOS ARM64/x86_64,
only. Build the CLI for Linux and both Mac architectures. Python
reuses all three C ABI libraries; JavaScript and C++ reuse Linux and both Mac
libraries. No client recompiles the shared compiler for packaging. Linux wheel
repair runs in local Docker.

The JVM release is `orchiddb-java-X.Y.Z.zip` on GitHub Releases, containing JVM
and Gremlin JARs, runtime dependencies, a file-based Maven repository, source and
Javadoc JARs, and all three JNI classifiers. Do not publish to Sonatype or Maven
Central.

Verification checks versions, architectures, source pins, and package hashes.
Publication uploads the locally verified assets and evidence to all nine GitHub
releases and compares uploaded bytes with local checksums. Language registry
publication is a separate upload step; it must reuse these same packages.
Never move published tags or replace published package bytes.

`pipeline.py` remains available to read older release state and validation logs.
Its workflow-dispatch build path is legacy and must not be used.

Windows is excluded from release builds and packages. The current Linux target
is x86_64; Linux ARM64 is not yet in the release matrix.
