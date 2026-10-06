# Local extension packaging

Run commands from the repository root. This workflow packages only the DuckDB
extension. It does not build clients, touch sibling repositories, or publish releases.

```sh
python3 -m venv extension/vendor/test-env
extension/vendor/test-env/bin/pip install -r extension/tests/requirements.txt
make -f scripts/release/Makefile build
make -f scripts/release/Makefile test
make -f scripts/release/Makefile package
```

Packaging copies the existing artifact and license into `target/packages/<sha256>`
and records its checksum and current source revision. Package only after building
and validating the intended source revision. This is an unsigned development
artifact, specific to its build platform and DuckDB 1.5.6; the manifest is not a
cross-platform certification. See [verification](../../docs/verification.md) for
upstream conformance commands. Publication and signing are separate operations.

All builds and tests run locally. Preserve completed artifacts and caches; never
replace a published artifact with different bytes. The old multi-repository client
and standalone CLI release pipeline has been retired from this repository.
