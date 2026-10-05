# Local release entry point. Compilation and packaging never run on GitHub.
RELEASE_DIR := $(abspath $(dir $(lastword $(MAKEFILE_LIST))))
WORKSPACE ?= $(abspath $(RELEASE_DIR)/../../..)
VERSION ?=
PYTHON ?= python3
RUSTUP_TOOLCHAIN ?= 1.93.1
export RUSTUP_TOOLCHAIN
ifeq ($(shell uname -s),Darwin)
JAVA_HOME ?= $(shell /usr/libexec/java_home -v 21 2>/dev/null)
export JAVA_HOME
endif
.DEFAULT_GOAL := help
.NOTPARALLEL:
.PHONY: help require-version plan build package verify publish release
help:
	@echo 'make release VERSION=0.3.0  Build locally, package, verify, then upload GitHub assets'
	@echo 'make build|package|verify|publish VERSION=0.3.0  Resume a completed stage'
	@echo 'Runtime tests are the separate integration.mk test target; releases reuse recorded validation.'
require-version:
	@test -n "$(VERSION)" || { echo 'Set VERSION (for example VERSION=0.3.0)'; exit 1; }
plan: require-version
	$(PYTHON) "$(RELEASE_DIR)/local_build.py" --version "$(VERSION)" --workspace "$(WORKSPACE)" --plan
build: require-version
	$(PYTHON) "$(RELEASE_DIR)/local_build.py" --version "$(VERSION)" --workspace "$(WORKSPACE)"
package: require-version
	$(PYTHON) "$(RELEASE_DIR)/local_package.py" package --version "$(VERSION)" --workspace "$(WORKSPACE)"
verify: require-version
	$(PYTHON) "$(RELEASE_DIR)/local_package.py" verify --version "$(VERSION)" --workspace "$(WORKSPACE)"
publish: verify
	$(PYTHON) "$(RELEASE_DIR)/local_publish.py" --version "$(VERSION)" --workspace "$(WORKSPACE)"
release: build package publish
