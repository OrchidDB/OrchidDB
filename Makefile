.DEFAULT_GOAL := help
.PHONY: help build cli native clients-check clients-test extension extension-test
help:
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
extension-test:
	$(MAKE) -f scripts/release/Makefile test
