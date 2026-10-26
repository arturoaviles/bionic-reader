SHELL := /bin/bash
.DEFAULT_GOAL := help
.DELETE_ON_ERROR:

CARGO ?= cargo
PYTHON ?= python3
NIGHTLY ?= nightly-2026-10-22
CARGO_TARGET_DIR ?= target
export CARGO_TARGET_DIR

.PHONY: help build release run install uninstall fmt fmt-check check lint test test-unit test-e2e test-doc doc verify static smoke completions completions-check coverage frameworks fuzz bench audit package clean

help: ## List common development commands.
	@awk 'BEGIN {FS = ":.*## "} /^[a-z][a-z0-9-]*:.*## / {printf "  make %-20s %s\n", $$1, $$2}' Makefile

build: ## Build the debug executable.
	$(CARGO) build --locked --bin bionic-reader

release: ## Build the optimized executable.
	$(CARGO) build --locked --release --bin bionic-reader

run: ## Run the CLI; pass arguments with ARGS='-t "reading"'.
	$(CARGO) run --locked --bin bionic-reader -- $(ARGS)

install: ## Install this checkout with Cargo (no registry publication).
	$(CARGO) install --path . --locked

uninstall: ## Remove the Cargo-installed executable.
	$(CARGO) uninstall bionic-reader

fmt: ## Format Rust source, tests, and benchmarks.
	$(CARGO) fmt --all
	$(CARGO) fmt --manifest-path fuzz/Cargo.toml --all

fmt-check: ## Check Rust formatting without changing files.
	$(CARGO) fmt --all -- --check
	$(CARGO) fmt --manifest-path fuzz/Cargo.toml --all -- --check

check: ## Type-check every Rust target.
	$(CARGO) check --locked --all-targets
	$(CARGO) check --locked --manifest-path fuzz/Cargo.toml --all-targets

lint: ## Run Clippy with warnings denied.
	$(CARGO) clippy --locked --all-targets -- -D warnings
	$(CARGO) clippy --locked --manifest-path fuzz/Cargo.toml --all-targets -- -D warnings

test: ## Run unit, integration, CLI, PTY, property, and benchmark smoke tests.
	$(CARGO) test --locked --all-targets

test-unit: ## Run library and CLI unit tests.
	$(CARGO) test --locked --lib --bins

test-e2e: ## Run executable, reader, shell-session, and completion workflows.
	$(CARGO) test --locked --test e2e --test reader --test session --test terminal --test completions

test-doc: ## Run Rustdoc examples.
	$(CARGO) test --locked --doc

doc: ## Build API documentation with warnings denied.
	RUSTDOCFLAGS="$(RUSTDOCFLAGS) -D warnings" $(CARGO) doc --locked --no-deps

verify: fmt-check check lint test test-doc doc completions-check static workflow-check docs-check ci-policy-check ## Run Rust, distribution, and fast offline CI checks.

static: ## Check shell/Python/Node/package sources (requires distribution tools).
	scripts/check-distribution.sh

smoke: build ## Exercise the built CLI and a real terminal session.
	$(PYTHON) scripts/check-installed-cli.py "$(CARGO_TARGET_DIR)/debug/bionic-reader"

completions: build ## Regenerate Bash, Zsh, and fish completion files.
	"$(CARGO_TARGET_DIR)/debug/bionic-reader" completions bash > completions/bionic-reader.bash
	"$(CARGO_TARGET_DIR)/debug/bionic-reader" completions zsh > completions/_bionic-reader
	"$(CARGO_TARGET_DIR)/debug/bionic-reader" completions fish > completions/bionic-reader.fish

completions-check: build ## Verify checked-in completions match the current CLI.
	scripts/check-completions.sh "$(CARGO_TARGET_DIR)/debug/bionic-reader"

coverage: ## Measure nightly line/branch coverage and write coverage HTML.
	CARGO_LLVM_COV_TARGET_DIR="$(CARGO_TARGET_DIR)/coverage-build" $(CARGO) +$(NIGHTLY) llvm-cov --branch --locked --all-targets --html --output-dir "$(CARGO_TARGET_DIR)/coverage"

frameworks: ## Fetch and test all seven pinned upstream shell frameworks.
	scripts/check-frameworks.sh

fuzz: ## Run the pinned nightly sanitizer fuzzer for 60 seconds.
	$(PYTHON) scripts/seed-fuzz-corpus.py
	$(CARGO) +$(NIGHTLY) fuzz run byte_stream -- -max_total_time=60 -max_len=131072 -timeout=10

bench: ## Run the transformer benchmarks.
	$(CARGO) bench --locked --bench transformer

audit: ## Check dependency advisories with the installed cargo-audit.
	$(CARGO) audit --deny warnings
	$(CARGO) audit --file fuzz/Cargo.lock --deny warnings

.PHONY: workflow-check docs-check docs-links deny sbom performance-report mutation-report ci-policy-check ci-negative-check

workflow-check: ## Lint YAML and workflow syntax/security (offline).
	actionlint -color
	yamllint -s .github .yamllint.yml
	zizmor --offline --format github .github/workflows

docs-check: ## Check tracked spelling and local Markdown links/anchors offline.
	git ls-files -z | xargs -0 typos --force-exclude
	scripts/check-docs.sh --offline

docs-links: ## Check tracked Markdown external links with bounded retries.
	scripts/check-docs.sh --external

ci-policy-check: ## Check toolchain/gate consistency and CI failure-path regressions.
	$(PYTHON) scripts/check-ci-policy.py
	$(PYTHON) -m unittest discover -s scripts/tests -v

ci-negative-check: ## Prove security, license, YAML and link tools reject invalid inputs.
	$(PYTHON) scripts/check-ci-negative.py

deny: ## Enforce license, source and version policies in both locked graphs.
	$(CARGO) deny --locked --config deny.toml check licenses bans sources
	$(CARGO) deny --manifest-path fuzz/Cargo.toml --locked --config deny.toml check licenses bans sources

sbom: ## Generate a target-specific binary SBOM; pass TARGET=target-triple.
	scripts/generate-sbom.sh "$(TARGET)" "$(CARGO_TARGET_DIR)/sbom/$(TARGET)"

performance-report: ## Record Divan, startup and binary-size measurements (advisory).
	scripts/performance-report.sh "$(CARGO_TARGET_DIR)/performance"

mutation-report: ## Run two mutation workers for up to 45 minutes; pass MODULE=settings|segmentation|transformer.
	$(PYTHON) scripts/mutation-report.py "$(MODULE)" --output "$(CARGO_TARGET_DIR)/mutations"

package: ## Verify/install/test/uninstall a Cargo archive from a clean checkout.
	scripts/check-cargo-package.sh

clean: ## Remove Cargo build outputs from CARGO_TARGET_DIR.
	$(CARGO) clean
