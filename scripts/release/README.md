# GitHub release packaging

`release.yml` builds the OrchidDB CLI on native runners and attaches four
archives, three explicitly labeled client placeholder ZIPs, and `SHA256SUMS` to a **draft GitHub release**. It runs on version-tag
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
   git tag -a v0.1.0 -m 'OrchidDB v0.1.0'
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
gh workflow run release.yml --repo OrchidDB/OrchidDB -f tag=v0.1.0
```

GitHub's regular source archives contain the repository and its vendored parser.
This workflow does not call `cargo package` or `cargo publish`.

## Local packaging

Python 3.11+ and a native Rust release build are required:

```sh
cargo build --locked --release --bin orchiddb
python3 scripts/release/package.py archive \
  --tag v0.1.0 --target aarch64-apple-darwin \
  --binary target/release/orchiddb
```

Choose the target matching your machine. The output is in
`target/release-packages/`; this local command smoke-checks the executable but
does not create a tag, push commits, or create a release.

## Cargo package name

The package, Rust import, and CLI are named `orchiddb`. Use a path or Git
dependency until a crate is published; no crates.io publication or name
reservation is part of this workflow. On 2026-09-25, the
[crates.io sparse index entry](https://index.crates.io/or/ch/orchiddb) returned
404, so no published crate was listed under this name. This is not a reservation.

Before publishing, resolve the local `vendor/spargebra` dependency: Cargo
replaces versioned path dependencies with registry dependencies when packaging,
so publishing this manifest unchanged would not ship the local parser changes.

The JVM bridge uses `io.orchiddb` packages and the `orchiddb-jvm-store` binary.
Rebuild Java artifacts together with the native engine after upgrading.

## Hosted installer

`https://install.orchiddb.com` serves `website/install/install.sh` from the private
`OrchidDB/OrchidDB-landing` repository. It selects a
published GitHub release (including prereleases), downloads the matching CLI
archive and `SHA256SUMS`, verifies the hash, then installs into `~/.local/bin`.
`ORCHIDDB_VERSION=v0.1.0` pins a release; `ORCHIDDB_INSTALL_DIR` overrides the
destination. Drafts are intentionally inaccessible to anonymous installers.
Publish the reviewed draft to activate downloads; no version has been fabricated
or published by the website deployment.

`mock_clients.py` generates deterministic Python, JavaScript/TypeScript, and Java
placeholder ZIPs with README and JSON metadata only. They are checksummed and
attached to GitHub releases, and hosted under `install.orchiddb.com/mock/` for
the landing-page preview. They are not disguised as wheels, npm packages, or
JARs. Replace them with actual SDK artifacts when those packages are released.

The install CDN uses its own CloudFront distribution, the existing private S3
bucket and wildcard certificate, and Route 53 A/AAAA aliases. Its root serves
the shell script as text/plain. In `OrchidDB/OrchidDB-landing`,
`website/scripts/provision-installer.py` can
reconcile DNS / create the distribution; `website/scripts/deploy.sh` publishes
script updates and mock assets and invalidates its cache.

Reference: [DuckDB installation](https://duckdb.org/docs/installation) and
[DuckDB's installer repository](https://github.com/duckdb/duckdb-install-scripts).

Tests: `python3 -m unittest discover -s scripts/release -p 'test_*.py'`.

Installer regression tests moved with the installer to `OrchidDB/OrchidDB-landing/tests/`. CLI archive packaging and its tests remain here.
