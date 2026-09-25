# GitHub release packaging

`release.yml` builds the Crabgraph CLI on native runners and attaches four
archives plus `SHA256SUMS` to a **draft GitHub release**. It runs on version-tag
pushes, or manually for an existing tag. It does not publish a crate to crates.io.

| Archive target | Runner | Format |
| --- | --- | --- |
| Linux x86_64, glibc 2.35+ | Ubuntu 22.04 | `.tar.gz` |
| macOS Intel | macOS 15 Intel | `.tar.gz` |
| macOS Apple Silicon | macOS 14 | `.tar.gz` |
| Windows x86_64 | Windows 2022, MSVC | `.zip` |

Rust is pinned to 1.93.1. The CLI builds with the default bundled DuckDB feature;
it needs no separately installed DuckDB library. Platform system libraries are
still required. Archives do not include the JVM bridge or Java dependencies.
macOS binaries are not Developer ID signed or notarized.

## Make a release

1. Commit the intended code, this workflow, and the packaging scripts.
2. Set the version in `Cargo.toml` and update `Cargo.lock` if the version changed.
3. Perform the project tests and conformance checks locally.
4. Review `scripts/release/NOTES.md` for the release.
5. Tag that commit with the matching version and push the tag:

   ```sh
   git tag -a v0.1.0 -m 'Crabgraph v0.1.0'
   git push origin v0.1.0
   ```

The workflow verifies the exact tag/manifest version, builds each platform,
runs `--help` and a `RETURN 1 AS value` smoke check, and packages the executable,
license, usage notes, and build provenance. Only when all four builds and their
checksums succeed does it create the draft. Versions starting with `0.` and
versions containing a prerelease suffix are marked as prereleases.

Review the resulting draft and publish it from GitHub when ready. Reruns can
replace assets on a draft; they refuse to alter an already published release.

To rerun an existing tag after the workflow is on the default branch:

```sh
gh workflow run release.yml --repo henneberger/new-graph -f tag=v0.1.0
```

GitHub's regular source archives contain the repository and its vendored parser.
This workflow does not call `cargo package` or `cargo publish`.

## Local packaging

Python 3.11+ and a native Rust release build are required:

```sh
cargo build --locked --release --bin crabgraph
python3 scripts/release/package.py archive \
  --tag v0.1.0 --target aarch64-apple-darwin \
  --binary target/release/crabgraph
```

Choose the target matching your machine. The output is in
`target/release-packages/`; this local command smoke-checks the executable but
does not create a tag, push commits, or create a release.

## Cargo package name

Recommended registry name: **`crabgraph-engine`** (Rust import convention:
`crabgraph_engine`). On 2026-09-25, the crates.io API returned 404 for both
`crabgraph-engine` and its underscore spelling. `crabgraph` is already used by
an unrelated cryptography library. `crabgraph-db` was also unregistered.

This is an availability check, not a reservation. The repository's existing
package remains `new-graph`, with Rust imports under `new_graph`; the binary
remains `crabgraph`. This workflow reads the real package name into `BUILD.json`
and works independently of a later package rename.

Before publishing to crates.io, adopt the chosen name and update its consumers.
Also resolve the local `vendor/spargebra` dependency: Cargo replaces versioned
path dependencies with registry dependencies when packaging, so publishing this
manifest unchanged would not ship the local parser changes. Keep the name
change and registry publication separate from these binary releases.

Sources:
- https://crates.io/api/v1/crates/crabgraph-engine
- https://crates.io/api/v1/crates/crabgraph
- https://doc.rust-lang.org/cargo/reference/publishing.html
- https://docs.github.com/en/actions/reference/runners/github-hosted-runners
