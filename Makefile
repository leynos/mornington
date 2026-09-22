.PHONY: help all clean test build release coverage lint fmt check-fmt markdownlint spelling nixie audit rust-audit

SHELL := bash


TARGET ?= mornington

USER_WHITAKER := $(HOME)/.local/bin/whitaker
USER_BIN_PATH := $(HOME)/.cargo/bin:$(HOME)/.local/bin:$(HOME)/.bun/bin
CARGO ?= cargo
BUILD_JOBS ?=
# RUSTFLAGS overrides .cargo/config.toml, so recipes that set it must re-state
# the Polonius flag explicitly.
POLONIUS_FLAGS ?= -Zpolonius=next
RUST_FLAGS ?=
RUST_FLAGS := -D warnings $(RUST_FLAGS)
DEV_LINKER_FLAGS ?= $(if $(filter Linux,$(shell uname -s)),-C link-arg=-fuse-ld=mold)
DEV_RUST_FLAGS ?= $(RUST_FLAGS) $(POLONIUS_FLAGS) $(DEV_LINKER_FLAGS)
RUSTDOC_FLAGS ?=
RUSTDOC_FLAGS := --cfg docsrs -D warnings $(POLONIUS_FLAGS) $(RUSTDOC_FLAGS)
CARGO_FLAGS ?= --all-targets --all-features
CLIPPY_FLAGS ?= $(CARGO_FLAGS) -- $(RUST_FLAGS)
TEST_FLAGS ?= $(CARGO_FLAGS)
TEST_CMD := $(if $(shell $(CARGO) nextest --version 2>/dev/null),nextest run,test)
COVERAGE_LINKER_FLAGS ?= -fuse-ld=lld
COVERAGE_RUST_FLAGS ?= $(RUST_FLAGS) $(POLONIUS_FLAGS) -C link-arg=$(COVERAGE_LINKER_FLAGS)
MDLINT ?= markdownlint-cli2
NIXIE ?= nixie
TYPOS_CONFIG_BUILDER_VERSION ?= v0.1.1
TYPOS_CONFIG_BUILDER = uv tool run --from \
	"git+https://github.com/leynos/typos-config-builder.git@$(TYPOS_CONFIG_BUILDER_VERSION)" \
	typos-config-builder
WHITAKER ?= $(or $(shell command -v whitaker 2>/dev/null),$(wildcard $(USER_WHITAKER)),whitaker)

build: target/debug/$(TARGET) ## Build debug binary
release: target/release/$(TARGET) ## Build release binary

all: check-fmt lint test ## Perform a comprehensive check of code
	+$(MAKE) spelling

clean: ## Remove build artefacts
	$(CARGO) clean
	rm -f .typos-oxendict-base.json .typos-oxendict-base.toml

test: export RUSTFLAGS := $(DEV_RUST_FLAGS)
test: ## Run tests with warnings treated as errors
	$(CARGO) $(TEST_CMD) $(TEST_FLAGS) $(BUILD_JOBS)
	RUSTDOCFLAGS="$(RUSTDOC_FLAGS)" $(CARGO) test --doc --workspace --all-features
ifeq ($(WITH_ACT),1)
	act pull_request --workflows .github/workflows/ci.yml --job build-test --platform ubuntu-latest=catthehacker/ubuntu:act-latest --secret GITHUB_TOKEN --env ACT=true
endif

target/%/$(TARGET): ## Build binary in debug or release mode
	RUSTFLAGS="$(DEV_RUST_FLAGS)" $(CARGO) build $(BUILD_JOBS) $(if $(findstring release,$(@)),--release) --bin $(TARGET)

coverage: ## Generate lcov coverage with lld for llvm-tools compatibility
	@echo "coverage linker flags: $(COVERAGE_LINKER_FLAGS)"
	CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER=clang \
		RUSTFLAGS="$(COVERAGE_RUST_FLAGS)" \
		CFLAGS="$(COVERAGE_LINKER_FLAGS)" \
		LDFLAGS="$(COVERAGE_LINKER_FLAGS)" \
		$(CARGO) llvm-cov --lcov --output-path lcov.info $(TEST_FLAGS)

lint: ## Run Clippy with warnings denied
	RUSTDOCFLAGS="$(RUSTDOC_FLAGS)" RUSTFLAGS="$(DEV_RUST_FLAGS)" $(CARGO) doc --no-deps
	RUSTFLAGS="$(DEV_RUST_FLAGS)" $(CARGO) clippy $(CLIPPY_FLAGS)
	@echo "Whitaker binary: $(WHITAKER)"
	PATH="$(USER_BIN_PATH):$(PATH)" RUSTFLAGS="$(DEV_RUST_FLAGS)" $(WHITAKER) --all -- $(CARGO_FLAGS)

typecheck: ## Type-check without building
	RUSTFLAGS="$(DEV_RUST_FLAGS)" $(CARGO) check $(CARGO_FLAGS)

fmt: ## Format Rust and Markdown sources
	$(CARGO) +nightly fmt --all
	mdformat-all

check-fmt: ## Verify formatting
	$(CARGO) fmt --all -- --check

markdownlint: ## Lint Markdown files
	$(MDLINT) '**/*.md'
	+$(MAKE) spelling
spelling: ## Enforce en-GB-oxendict spelling in Markdown prose
	@if ! git rev-parse --is-inside-work-tree >/dev/null 2>&1; then \
		echo "make spelling needs a Git repository: the gate enumerates tracked files with git ls-files. Run: git init && git add -A" >&2; \
		exit 1; \
	fi
	@if [ -z "$$(git ls-files)" ]; then \
		echo "make spelling found no tracked files: the gate enumerates tracked files with git ls-files. Run: git add -A" >&2; \
		exit 1; \
	fi
	$(TYPOS_CONFIG_BUILDER) gate --repository .

nixie: ## Validate Mermaid diagrams
	$(NIXIE) --no-sandbox

audit: rust-audit ## Audit dependencies for known vulnerabilities

rust-audit: ## Audit the Rust workspace for known vulnerabilities
	set -eo pipefail; \
	manifest_list=$$(mktemp); \
	trap 'rm -f "$$manifest_list"' EXIT; \
	printf "Audit metadata phase: deriving workspace manifests\n"; \
	$(CARGO) metadata --no-deps --format-version 1 | python3 -c 'import json, sys; metadata = json.load(sys.stdin); members = set(metadata["workspace_members"]); print(metadata["workspace_root"]); [print(package["manifest_path"]) for package in metadata["packages"] if package["id"] in members]' > "$$manifest_list"; \
	workspace_root=$$(sed -n '1p' "$$manifest_list"); \
	audit_flags=(); \
	for advisory in $$CARGO_AUDIT_IGNORES; do \
		audit_flags+=(--ignore "$$advisory"); \
	done; \
	printf "Auditing Rust workspace %s\n" "$$workspace_root"; \
	sed -n '2,$$p' "$$manifest_list" | while IFS= read -r manifest; do \
		manifest_dir=$$(dirname "$$manifest"); \
		printf "Workspace Rust manifest %s\n" "$$manifest_dir/Cargo.toml"; \
	done; \
	printf "Audit execution phase: running cargo audit\n"; \
	printf "Audit failures may indicate RustSec advisories, cargo metadata errors, or documented ignores that need CARGO_AUDIT_IGNORES entries.\n"; \
	(cd "$$workspace_root" && $(CARGO) audit "$${audit_flags[@]}")

help: ## Show available targets
	@grep -E '^[a-zA-Z_-]+:.*?##' $(MAKEFILE_LIST) | \
	awk 'BEGIN {FS=":"; printf "Available targets:\n"} {printf "  %-20s %s\n", $$1, $$2}'
