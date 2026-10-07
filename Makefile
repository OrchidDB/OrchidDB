.DEFAULT_GOAL := help
PYTHON ?= python3
TEST_PYTHON ?= extension/vendor/test-env/bin/python
VERSION ?= $(shell $(PYTHON) -c 'import tomllib; print(tomllib.load(open("Cargo.toml", "rb"))["package"]["version"])')
RELEASE_PYTHON ?= target/release-env/bin/python
export TEST_PYTHON RELEASE_PYTHON
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
    (name.startswith(('cli/', 'clients/')) and Path(name).name in
     ('Cargo.toml', 'package.json', 'package-lock.json', 'pyproject.toml', 'pom.xml', 'mix.exs', 'CMakeLists.txt'))]
for name in manifests:
    path = Path(name)
    text = path.read_text()
    updated = text.replace(old, version)
    if updated != text:
        path.write_text(updated)
for name in ('Cargo.lock', 'extension/compiler/Cargo.lock'):
    path = Path(name)
    text = path.read_text()
    updated = re.sub(r'(name = "orchiddb[^"\n]*"\nversion = ")[^"]+(")', lambda m: m[1] + version + m[2], text)
    if updated != text:
        path.write_text(updated)
if subprocess.check_output(['git', 'diff', '--name-only']):
    subprocess.run(['git', 'add', '--', *manifests, 'Cargo.lock', 'extension/compiler/Cargo.lock'], check=True)
    subprocess.run(['git', 'commit', '-m', 'Prepare ' + version + ' release [skip ci]'], check=True)
endef
export RELEASE_VERSION_SCRIPT

.PHONY: help build cli native clients-check clients-test extension extension-test
help:
	@echo 'make release        Build and package every GitHub release asset locally'
	@echo 'make build          Build the core library'
	@echo 'make cli            Build the standalone CLI'
	@echo 'make native         Build and stage the shared client library'
	@echo 'make clients-check  Check Rust client, JNI, native binding and CLI'
	@echo 'make clients-test   Test Rust client, native binding and CLI locally'
	@echo 'make extension      Build the optional DuckDB extension'
	@echo 'See clients/README.md for language-specific build and test commands.'
build:
	cargo build --locked -p orchiddb
cli:
	cargo build --locked -p orchiddb-cli
native:
	python3 scripts/clients.py native
clients-check:
	cargo check --locked -p orchiddb-client -p orchiddb-compiler-native -p orchiddb-java-native -p orchiddb-cli
clients-test:
	cargo test --locked -p orchiddb-client --features bundled-test-driver -p orchiddb-compiler-native -p orchiddb-cli
extension:
	python3 extension/scripts/build.py
extension-release:
	$(PYTHON) extension/scripts/build.py --release --extension-version "$(VERSION)"
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
