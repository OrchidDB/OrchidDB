PYTHON ?= python3
TEST_PYTHON ?= extension/vendor/test-env/bin/python
VERSION ?=
REPO ?= OrchidDB/OrchidDB
NOTES_FILE ?=
RELEASE_ARGS = --version "$(VERSION)" --repo "$(REPO)" $(if $(NOTES_FILE),--notes-file "$(NOTES_FILE)")
.PHONY: build test test-release package release-check release publish
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

# Keep stages ordered even with make -j. Publish only after local tests pass.
release:
	+$(MAKE) -f scripts/release/Makefile release-check
	+$(MAKE) -f scripts/release/Makefile build
	+$(MAKE) -f scripts/release/Makefile test
	+$(MAKE) -f scripts/release/Makefile publish

# Reuse an already built and validated artifact without repeating build/tests.
publish:
	$(PYTHON) scripts/release/publish_extension.py $(RELEASE_ARGS)
