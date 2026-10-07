.DEFAULT_GOAL := help
PYTHON ?= python3
TEST_PYTHON ?= extension/vendor/test-env/bin/python
VERSION ?= $(shell $(PYTHON) -c 'import tomllib; print(tomllib.load(open("Cargo.toml", "rb"))["package"]["version"])')
export TEST_PYTHON

.PHONY: help build cli native clients-check clients-test extension extension-test
help:
	@echo 'make release        Build and verify every GitHub release asset locally'
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

.PHONY: extension-release extension-package release-tools-test release release-check release-test release-build release-package release-verify

# All builds/checks are local. The final directory is ready for manual GitHub upload.
release:
	$(PYTHON) scripts/release/release.py all --version "$(VERSION)"
release-check:
	$(PYTHON) scripts/release/release.py check --version "$(VERSION)"
release-test:
	$(PYTHON) scripts/release/release.py test --version "$(VERSION)"
release-build:
	$(PYTHON) scripts/release/release.py build --version "$(VERSION)"
release-package:
	$(PYTHON) scripts/release/release.py package --version "$(VERSION)"
release-verify:
	$(PYTHON) scripts/release/release.py verify --version "$(VERSION)"
