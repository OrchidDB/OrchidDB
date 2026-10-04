# Local cross-repository integration checks. Invoke from anywhere with make -C ~/orchiddb.
ROOT := $(abspath $(dir $(lastword $(MAKEFILE_LIST)))/../../..)
INTEGRATION := $(ROOT)/orchiddb/scripts/release/integration
CARGO_TARGET_DIR ?= $(ROOT)/target/integration
CARGO_PROFILE_DEV_DEBUG ?= 0
RUST_MIN_STACK ?= 16777216
export CARGO_TARGET_DIR CARGO_PROFILE_DEV_DEBUG RUST_MIN_STACK

# Select a JDK for these tests without changing the caller's shell environment.
ifeq ($(shell uname -s),Darwin)
INTEGRATION_JAVA_HOME ?= $(shell /usr/libexec/java_home -v 21 2>/dev/null)
else
INTEGRATION_JAVA_HOME ?= $(JAVA_HOME)
endif
JAVA_ENV = $(if $(INTEGRATION_JAVA_HOME),JAVA_HOME="$(INTEGRATION_JAVA_HOME)",)

# An application-owned external DuckDB installation can replace bundled builds.
DRIVER_FLAGS := $(if $(DUCKDB_LIB_DIR),--no-default-features,)

.PHONY: help test integration-test java-test compiler-check java-compiler-check rust-test cli-test native-test python-test elixir-test js-test cpp-test
.NOTPARALLEL:
.DEFAULT_GOAL := help

help:
	@echo 'make test              All compiler, CLI, and language-client integration suites'
	@echo 'make integration-test  Real caller-owned DuckDB tests'
	@echo 'make java-test         Java client and Gremlin module against the sibling Rust checkout'
	@echo 'make compiler-check    Assert the compiler has no database driver dependency'
	@echo 'make java-compiler-check  Assert the JNI library has no database driver dependency'
	@echo 'See INTEGRATION.md for prerequisites, external DuckDB, and build-cache settings.'

test: integration-test java-test rust-test cli-test native-test python-test elixir-test js-test cpp-test
	@echo 'All OrchidDB integration suites passed.'

compiler-check:
	@cargo tree --locked --manifest-path "$(ROOT)/orchiddb/Cargo.toml" --edges normal --prefix none > "$(INTEGRATION)/compiler-dependencies.txt"
	@while IFS= read -r dependency; do \
	  case "$$dependency" in duckdb\ *|libduckdb-sys\ *|postgres\ *|tokio-postgres\ *) \
	    echo "Unexpected compiler driver dependency: $$dependency"; exit 1;; esac; \
	done < "$(INTEGRATION)/compiler-dependencies.txt"
	@echo 'Compiler dependency boundary passed (no DuckDB/PostgreSQL driver).'

integration-test: compiler-check
	cargo test --locked --manifest-path "$(INTEGRATION)/Cargo.toml" $(DRIVER_FLAGS) --test end_to_end -- --test-threads=1

java-compiler-check:
	@cargo tree --locked --manifest-path "$(ROOT)/orchiddb-java/native/Cargo.toml" --edges normal --prefix none > "$(INTEGRATION)/java-native-dependencies.txt"
	@while IFS= read -r dependency; do \
	  case "$$dependency" in duckdb\ *|libduckdb-sys\ *|postgres\ *|tokio-postgres\ *) \
	    echo "Unexpected JNI driver dependency: $$dependency"; exit 1;; esac; \
	done < "$(INTEGRATION)/java-native-dependencies.txt"
	@echo 'JNI dependency boundary passed (no DuckDB/PostgreSQL driver).'

java-test: java-compiler-check
	cd "$(ROOT)/orchiddb-java" && $(JAVA_ENV) bash scripts/build.sh -Pgremlin dependency:tree -Dscope=runtime -DoutputFile=target/runtime-dependencies.txt
	@for module in orchiddb-java orchiddb-gremlin; do \
	  report="$(ROOT)/orchiddb-java/$$module/target/runtime-dependencies.txt"; \
	  test -s "$$report" || { echo "Missing runtime dependency report: $$report"; exit 1; }; \
	  while IFS= read -r dependency; do \
	    case "$$dependency" in *org.duckdb:*) \
	      echo "DuckDB JDBC leaked into $$module runtime dependencies: $$dependency"; exit 1;; esac; \
	  done < "$$report"; \
	done
	@echo 'Java runtime dependency boundary passed (DuckDB JDBC remains test-only).'

rust-test:
	cargo test --locked --manifest-path "$(ROOT)/orchiddb-rust/Cargo.toml" $(DRIVER_FLAGS) --features quickwit,elasticsearch

cli-test:
	cargo test --locked --manifest-path "$(ROOT)/orchiddb-cli/Cargo.toml" $(DRIVER_FLAGS) --features quickwit,elasticsearch

# All foreign clients share this compiler-only ABI; no result data crosses it.
NATIVE_SUFFIX := $(if $(filter Darwin,$(shell uname -s)),dylib,so)
ORCHIDDB_NATIVE_LIBRARY ?= $(CARGO_TARGET_DIR)/debug/liborchiddb_compiler.$(NATIVE_SUFFIX)
export ORCHIDDB_NATIVE_LIBRARY
PYTHON ?= python3
# Homebrew's keg-only installs are supported without changing the caller's PATH.
ELIXIR_PATH := /opt/homebrew/opt/erlang/bin:/opt/homebrew/opt/elixir/bin:$(PATH)

native-test:
	cargo test --locked --manifest-path "$(ROOT)/orchiddb-native/Cargo.toml"
	cargo build --locked --manifest-path "$(ROOT)/orchiddb-native/Cargo.toml"
	$(PYTHON) "$(ROOT)/orchiddb-native/scripts/smoke.py" "$(ORCHIDDB_NATIVE_LIBRARY)"

python-test: native-test
	cd "$(ROOT)/orchiddb-python" && $(PYTHON) -m venv .venv
	cd "$(ROOT)/orchiddb-python" && .venv/bin/python -m pip install -e '.[test]'
	cd "$(ROOT)/orchiddb-python" && .venv/bin/python -m pytest -q

elixir-test: native-test
	cd "$(ROOT)/orchiddb-elixir" && PATH="$(ELIXIR_PATH)" mix deps.get
	cd "$(ROOT)/orchiddb-elixir" && PATH="$(ELIXIR_PATH)" mix test

js-test: native-test
	cd "$(ROOT)/orchiddb-js" && npm ci
	cd "$(ROOT)/orchiddb-js" && npm test

cpp-test: native-test
	cd "$(ROOT)/orchiddb-cpp" && bash scripts/test.sh
