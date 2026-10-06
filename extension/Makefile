.PHONY: build release test test-storage

build:
	python3 scripts/build.py

release:
	python3 scripts/build.py --release

test: build
	CARGO_TARGET_DIR="$(CURDIR)/../target" cargo test --locked --manifest-path compiler/Cargo.toml --lib
	vendor/test-env/bin/python -m unittest discover -s tests -p 'test_*.py'

test-storage: build
	ORCHID_EXTERNAL_TESTS=1 vendor/test-env/bin/python -m unittest discover -s tests -p 'test_*.py'
