PYTHON ?= python3
TEST_PYTHON ?= extension/vendor/test-env/bin/python
VERSION ?=
REPO ?= OrchidDB/OrchidDB
NOTES_FILE ?=
PLATFORM ?=
RELEASE_ARGS = --version "$(VERSION)" --repo "$(REPO)" $(if $(NOTES_FILE),--notes-file "$(NOTES_FILE)")
MATRIX_ARGS = --version "$(VERSION)" $(if $(PLATFORM),--platform "$(PLATFORM)")
.PHONY: build test test-release package release-check release release-build release-verify release-package release-test release-publish publish
build:
	$(PYTHON) extension/scripts/build.py --release $(if $(VERSION),--extension-version "$(VERSION)")

test: test-release
	CARGO_TARGET_DIR="$(CURDIR)/target" cargo test --locked --manifest-path extension/compiler/Cargo.toml --lib
	ORCHID_EXTERNAL_TESTS=1 $(TEST_PYTHON) -m unittest discover -s extension/tests -p 'test_*.py'

test-release:
	$(PYTHON) -m unittest discover -s scripts/release -p 'test_*.py' -v

package:
	$(PYTHON) scripts/release/package_extension.py

release-check:
	$(PYTHON) scripts/release/publish_extension.py $(RELEASE_ARGS) --check
	$(PYTHON) scripts/release/matrix.py check $(MATRIX_ARGS)

release-build:
	$(PYTHON) scripts/release/matrix.py build $(MATRIX_ARGS)

release-package:
	$(PYTHON) scripts/release/matrix.py package --version "$(VERSION)"

release-verify: release-package
release-test: test
release-publish: publish

# Builds run concurrently inside matrix.py; publish after all three finish.
release:
	+$(MAKE) -f scripts/release/Makefile release-check
	+$(MAKE) -f scripts/release/Makefile release-build PLATFORM=
	+$(MAKE) -f scripts/release/Makefile publish

# Reuse an already built and validated artifact without repeating build/tests.
publish:
	$(PYTHON) scripts/release/publish_extension.py $(RELEASE_ARGS)
