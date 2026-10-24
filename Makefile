.DEFAULT_GOAL := help
PYTHON ?= python3
TEST_PYTHON ?= extension/vendor/test-env/bin/python
VERSION ?= $(shell $(PYTHON) -c 'import tomllib; print(tomllib.load(open("Cargo.toml", "rb"))["package"]["version"])')
RELEASE_PYTHON ?= target/release-env/bin/python
RELEASE_JOBS ?= 3
export TEST_PYTHON RELEASE_PYTHON RELEASE_JOBS
export VERSION

define RELEASE_VERSION_SCRIPT
import os, re, subprocess, tomllib
from pathlib import Path
version = os.environ['VERSION']
if len(version) > 31 or not re.fullmatch(r'\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?', version):
    raise SystemExit('VERSION must be a semantic version, for example 0.4.0')
if subprocess.check_output(['git', 'status', '--porcelain']):
    raise SystemExit('Release requires a clean checkout; existing edits will not be committed automatically')
old = tomllib.loads(Path('Cargo.toml').read_text())['package']['version']
names = subprocess.check_output(['git', 'ls-files'], text=True).splitlines()
manifests = [name for name in names if name == 'Cargo.toml' or
    (name.startswith('clients/') and Path(name).name in
     ('Cargo.toml', 'package.json', 'package-lock.json', 'pyproject.toml', 'pom.xml', 'mix.exs', 'CMakeLists.txt'))]
for name in manifests:
    path = Path(name)
    text = path.read_text()
    updated = text.replace(old, version)
    if updated != text:
        path.write_text(updated)
for name in ('Cargo.lock',):
    path = Path(name)
    text = path.read_text()
    updated = re.sub(r'(name = "orchiddb[^"\n]*"\nversion = ")[^"]+(")', lambda m: m[1] + version + m[2], text)
    if updated != text:
        path.write_text(updated)
if subprocess.check_output(['git', 'diff', '--name-only']):
    subprocess.run(['git', 'add', '--', *manifests, 'Cargo.lock'], check=True)
    subprocess.run(['git', 'commit', '-m', 'Prepare ' + version + ' release [skip ci]'], check=True)
endef
export RELEASE_VERSION_SCRIPT

.PHONY: help build native clients-check clients-test go-check go-test extension extension-test
help:
	@echo 'make release        Build locally and publish all GitHub release assets'
	@echo 'make build          Build the core library'
	@echo 'make native         Build and stage the shared client library'
	@echo 'make catalog-smoke  Smoke catalog auth and all clients on DuckDB'
	@echo 'make clients-check  Check Rust client, JNI, and native binding'
	@echo 'make clients-test   Test Rust client and native binding locally'
	@echo 'make go-check       Build and vet the Go client'
	@echo 'make go-test        Test the Go client with the race detector'
	@echo 'make starrocks-test Start local StarRocks and check query execution'
	@echo 'make starrocks-conformance Run all language catalogs on local StarRocks'
	@echo 'make conformance-report Build the HTML comparison report from recorded results'
	@echo 'make extension      Build the optional DuckDB extension'
	@echo 'See clients/README.md for language-specific build and test commands.'
build:
	cargo build --locked -p orchiddb
native:
	python3 scripts/clients.py native
clients-check:
	cargo check --locked -p orchiddb-client -p orchiddb-compiler-native -p orchiddb-java-native
clients-test:
	cargo test --locked -p orchiddb-client --features bundled-test-driver -p orchiddb-compiler-native
go-check:
	cd clients/go && CGO_ENABLED=0 go build ./... && CGO_ENABLED=0 go vet ./...
go-test:
	cd clients/go && CGO_ENABLED=0 go test ./...
	# External linking supports the macOS loader with Go 1.23 race binaries.
	cd clients/go && CGO_ENABLED=1 go test -race -ldflags=-linkmode=external ./...
extension:
	python3 extension/scripts/build.py
extension-release:
	$(PYTHON) extension/scripts/build.py --release --extension-version "$(VERSION)" $(EXTENSION_BUILD_ARGS)
extension-test: release-tools-test
	CARGO_TARGET_DIR="$(CURDIR)/target" cargo test --locked --manifest-path extension/compiler/Cargo.toml --lib
	ORCHID_EXTERNAL_TESTS=1 $(TEST_PYTHON) -m unittest discover -s extension/tests -p 'test_*.py'
extension-package:
	$(PYTHON) scripts/release/package_extension.py
release-tools-test:
	$(PYTHON) -m unittest discover -s scripts/release -p 'test_*.py' -v

.PHONY: extension-release extension-package release-tools-test release release-prepare release-env release-check release-build release-package

release:
	$(MAKE) release-prepare
	$(MAKE) release-env
	$(PYTHON) scripts/release/release.py all --version "$(VERSION)"
release-prepare:
	$(PYTHON) -c "$$RELEASE_VERSION_SCRIPT"
release-env:
	@test -x "$(RELEASE_PYTHON)" || $(PYTHON) -m venv "$$(dirname "$(RELEASE_PYTHON)")/.."
	$(RELEASE_PYTHON) -m pip install build wheel setuptools
release-check:
	$(PYTHON) scripts/release/release.py check --version "$(VERSION)"
release-build:
	$(PYTHON) scripts/release/release.py build --version "$(VERSION)"
release-package:
	$(PYTHON) scripts/release/release.py package --version "$(VERSION)"

.PHONY: release-publish
release-publish:
	$(PYTHON) scripts/release/release.py publish --version "$(VERSION)"

.PHONY: starrocks-test
starrocks-test:
	$(PYTHON) tools/diagnostics/starrocks_test.py

.PHONY: starrocks-conformance
starrocks-conformance:
	$(PYTHON) tools/diagnostics/starrocks_test.py --conformance

.PHONY: catalog-smoke
catalog-smoke:
	$(TEST_PYTHON) tools/diagnostics/catalog_smoke.py $(CATALOG_SMOKE_ARGS)

REPORT_RESULTS ?= conformance/upstream-results
REPORT_OUTPUT ?= target/conformance/report
.PHONY: conformance-report
conformance-report:
	$(PYTHON) conformance/report/build.py --results "$(REPORT_RESULTS)" --output "$(REPORT_OUTPUT)"

.PHONY: trino-test
trino-test:
	$(PYTHON) tools/trino/setup.py
	ORCHIDDB_TEST_TRINO=1 ORCHIDDB_TEST_TRINO_CONNECTORS=1 cargo test --locked --features prometheus,loki --test trino
	ORCHIDDB_TEST_TRINO=1 ORCHIDDB_TEST_PYTHON="$(TEST_PYTHON)" cargo test --locked --test temporal_edges

.PHONY: duckdb-observability-test
duckdb-observability-test:
	ORCHIDDB_TEST_DUCKDB_OBSERVABILITY=1 ORCHIDDB_TEST_PYTHON="$${ORCHIDDB_TEST_PYTHON:-$(PYTHON)}" cargo test --locked --test duckdb_observability
