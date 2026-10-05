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

The builder assembles one Cargo workspace around the pinned source checkouts,
with one committed `workspace.Cargo.lock` and one release profile. Each platform
uses **one Cargo invocation** selecting the C ABI, JNI, and CLI together, so Cargo
shares their dependency compilation. All three resolve the core from the same
pinned local checkout. CLI C++ link settings apply only to the CLI. Source tags
are not changed; only build manifests are generated under the release directory.

A missing output resumes the same package and feature selection; Cargo reuses
completed work. Packaging Python, Node, and C++ does not invoke Rust compilation.
To inspect the four build commands without compiling or downloading toolchains:

```sh
make release-plan VERSION=X.Y.Z
```

On macOS, the local builder uses Xcode for both Mac architectures, cargo-zigbuild
for Linux x86_64 and ARM64 (`aarch64-unknown-linux-gnu.2.34`). Missing Rust
targets are installed automatically. Cross-tooling lives
in `.releases/tooling`; persistent output and SDK caches live under `target/`.
The CLI links checksum-pinned official DuckDB static libraries. Its Linux C++
sysroots are prepared in local Docker, separately for amd64 and arm64.
You can prepare one manually with `scripts/release/local_linux_sdk.sh /path/to/workspace aarch64`
(or `x86_64`). Both Linux targets use glibc 2.34.

Build the shared C ABI, JNI libraries, and CLI for Linux ARM64/x86_64 and
macOS ARM64/x86_64. Python, JavaScript, and C++ reuse all four C ABI libraries. No client recompiles the shared compiler for packaging. Linux wheel
repair runs in local Docker.

The JVM release is `orchiddb-java-X.Y.Z.zip` on GitHub Releases, containing JVM
and Gremlin JARs, runtime dependencies, a file-based Maven repository, source and
Javadoc JARs, and all four JNI classifiers. Do not publish to Sonatype or Maven
Central.

Verification checks versions, architectures, source pins, and package hashes.
Publication uploads the locally verified assets and evidence to all nine GitHub
releases and compares uploaded bytes with local checksums. Language registry
publication is a separate upload step; it must reuse these same packages.
Never move published tags or replace published package bytes.

`pipeline.py` remains available to read older release state and validation logs.
Its workflow-dispatch build path is legacy and must not be used.

Windows is excluded from release builds and packages.
