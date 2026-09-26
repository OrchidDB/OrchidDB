# Rust release preparation

The engine `orchiddb` and client `orchiddb-client` are version 0.1.0.
The modified SPARQL parser ships as engine source in `src/spargebra`, with
its upstream license notices. There is no separate parser crate.

Both repositories have a manual `release.yml` workflow. Run with `ref=main`
and leave `publish=false` to test, build, and verify downloadable `.crate`
artifacts and checksums. This creates no tag, GitHub release, or registry upload.
The client checks out its exact engine revision and verifies both packages in a
temporary Cargo workspace, so preparation works before either crate is published.

When publication is authorized later, commit and review both repositories, update
the client's engine revision, and create matching v0.1.0 tags. Run the engine
workflow with `ref=v0.1.0` and `publish=true` first. Once crates.io exposes that
version, run the client workflow with the same inputs. Each workflow uploads
only its own verified crate. Publishing requires `CARGO_REGISTRY_TOKEN` in its
repository's `crates-io` environment. Consider enabling environment reviewers.
Do not change a version's source or tag after publishing it.

Local verification (no registry writes):

```sh
python3 scripts/release/package-workspace.py --core . --output /tmp/orchiddb-crates --test
python3 scripts/release/package-workspace.py --core . --rust ../orchiddb-rust --output /tmp/orchiddb-crates --test
```

The CLI stays `publish=false` in `OrchidDB/OrchidDB-cli`. Its native binaries
are distributed through GitHub Releases and install.orchiddb.com. This engine
workflow no longer builds legacy CLI archives or placeholder client downloads.
