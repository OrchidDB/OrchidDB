PYTHON ?= python3
TEST_PYTHON ?= extension/vendor/test-env/bin/python
.PHONY: build test package
build:
	$(PYTHON) extension/scripts/build.py --release

test:
	CARGO_TARGET_DIR="$(CURDIR)/target" cargo test --locked --manifest-path extension/compiler/Cargo.toml --lib
	ORCHID_EXTERNAL_TESTS=1 $(TEST_PYTHON) -m unittest discover -s extension/tests -p 'test_*.py'

package:
	$(PYTHON) scripts/release/package_extension.py
