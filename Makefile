# `make fmt` and `make check-fmt` call mdtablefix directly. `--git` selects the
# Markdown files Git tracks and `--include-untracked` adds the untracked files
# Git does not ignore, so a new document is formatted before it is staged.
# Both modes need mdtablefix 0.6.0 or later; CI pins the version at its
# install-mdtablefix step.
MDTABLEFIX ?= mdtablefix
MDTABLEFIX_SELECT = --git --include-untracked
MDTABLEFIX_RULES = --wrap --renumber --breaks --ellipsis --fences

.PHONY: help all clean test test-python build release coverage lint lint-clippy lint-whitaker lint-python typecheck typecheck-rust typecheck-python fmt check-fmt markdownlint spelling nixie audit rust-audit test-workflow-contracts install-build-tools check-build-tools check-coverage-tools

# The public composite target recurses one gate at a time, even when callers
# pass `-j`, so shared build caches never receive overlapping gate work.
.NOTPARALLEL: all lint typecheck test test-workflow-contracts

SHELL := bash


TARGET ?= mornington

USER_WHITAKER := $(HOME)/.local/bin/whitaker
USER_BIN_PATH := $(HOME)/.cargo/bin:$(HOME)/.local/bin:$(HOME)/.bun/bin
BUILD_TOOLS_PREFIX ?= $(HOME)/.local
export BUILD_TOOLS_PREFIX
export PATH := $(BUILD_TOOLS_PREFIX)/bin:$(USER_BIN_PATH):$(PATH)
CARGO ?= cargo
BUILD_JOBS ?=
# RUSTFLAGS overrides .cargo/config.toml, so recipes that set it must re-state
# the Polonius flag explicitly.
POLONIUS_FLAGS ?= -Zpolonius=next
RUST_FLAGS ?=
RUST_FLAGS := -D warnings $(RUST_FLAGS)
# The development build standard (concordat rule `rust-build-defaults`): the
# parallel rustc frontend and, on Linux, the mold linker. Recipes compose these
# onto any inherited RUSTFLAGS (CI's setup-rust exports one), because an
# assigned RUSTFLAGS replaces every `rustflags` table in .cargo/config.toml.
STANDARD_THREADS_FLAG ?= -Zthreads=8
BUILD_HOST_OS := $(shell uname -s)
DEV_LINKER_FLAGS ?= $(if $(filter Linux,$(BUILD_HOST_OS)),-C link-arg=-fuse-ld=mold)
DEV_RUST_FLAGS ?= $(RUST_FLAGS) $(POLONIUS_FLAGS) $(STANDARD_THREADS_FLAG) $(DEV_LINKER_FLAGS)
RUSTDOC_FLAGS ?=
RUSTDOC_FLAGS := --cfg docsrs -D warnings $(POLONIUS_FLAGS) $(RUSTDOC_FLAGS)
CARGO_FLAGS ?= --all-targets --all-features
CLIPPY_FLAGS ?= --workspace $(CARGO_FLAGS) -- $(RUST_FLAGS)
TEST_FLAGS ?= $(CARGO_FLAGS)
TEST_CMD := $(if $(shell $(CARGO) nextest --version 2>/dev/null),nextest run,test)
COVERAGE_LINKER_FLAGS ?= -fuse-ld=lld
COVERAGE_RUST_FLAGS ?= $(RUST_FLAGS) $(POLONIUS_FLAGS) -C link-arg=$(COVERAGE_LINKER_FLAGS)
MDLINT ?= markdownlint-cli2
NIXIE ?= nixie
TYPOS_CONFIG_BUILDER_VERSION = v0.1.3
TYPOS_CONFIG_BUILDER = uv tool run --from \
	"git+https://github.com/leynos/typos-config-builder.git@$(TYPOS_CONFIG_BUILDER_VERSION)" \
	typos-config-builder
WHITAKER ?= $(or $(shell command -v whitaker 2>/dev/null),$(wildcard $(USER_WHITAKER)),whitaker)
INSTALL_BUILD_TOOLS ?= scripts/install-build-tools.sh
CHECK_BUILD_TOOLS ?= scripts/check-build-tools.sh
UV ?= uv
UV_ENV = UV_CACHE_DIR=.uv-cache UV_TOOL_DIR=.uv-tools

# All repository-owned Python uses the same managed CPython baseline. Keep
# this value aligned with Ruff and Pylint in pyproject.toml; the workflow
# contract rejects drift between the three sources.
PYTHON_BASELINE ?= 3.14
PYTHON = $(UV_ENV) $(UV) run --no-project --managed-python \
	--python $(PYTHON_BASELINE) python
RUFF_VERSION ?= 0.16.4
RUFF = $(UV_ENV) $(UV) tool run --managed-python \
	--python $(PYTHON_BASELINE) --from ruff==$(RUFF_VERSION) ruff
PYLINT_VERSION ?= 4.0.9
DF12_PYTHON_LINTS_REF ?= 4cf41736cce2f7ba2778882a5c629c044568a0e5
DF12_PYTHON_LINTS = git+https://github.com/leynos/df12-python-lints.git@$(DF12_PYTHON_LINTS_REF)
DF12_PYLINT_MESSAGES = R9101,C9102,R9103,R9104,C9105,C9106,C9107,R9108,R9109,R9110,R9111,R9112,C9112
# Pylint's defaults remain enabled while the same process loads the full
# pinned df12 policy. No `--disable` tier can hide a finding from either set.
PYLINT = $(UV_ENV) $(UV) tool run --managed-python \
	--python $(PYTHON_BASELINE) --from 'pylint==$(PYLINT_VERSION)' \
	--with '$(DF12_PYTHON_LINTS)' pylint \
	--load-plugins=df12_python_lints --enable=$(DF12_PYLINT_MESSAGES)
AMBRLEAKS = $(UV_ENV) $(UV) tool run --managed-python \
	--python $(PYTHON_BASELINE) --from '$(DF12_PYTHON_LINTS)' ambrleaks
INTERROGATE_VERSION ?= 1.7.0
INTERROGATE = $(UV_ENV) $(UV) tool run --managed-python \
	--python $(PYTHON_BASELINE) --from 'interrogate==$(INTERROGATE_VERSION)' \
	interrogate --fail-under 100
TY_VERSION ?= 0.0.74
PYTHON_SOURCE_ROOTS ?= .github tests scripts benches benchmarks
PYTHON_EXISTING_SOURCE_ROOTS = $(wildcard $(PYTHON_SOURCE_ROOTS))
PYTHON_PRUNED_DIRECTORIES = \
	-name .git -prune -o -name .venv -prune -o -name venv -prune -o \
	-name .uv-cache -prune -o -name .uv-tools -prune -o \
	-name target -prune -o -name vendor -prune -o -name node_modules -prune -o \
	-name __pycache__ -prune -o -name .pytest_cache -prune -o \
	-name .mypy_cache -prune -o -name .ruff_cache -prune -o
PYTHON_SOURCES = $(strip $(shell find $(PYTHON_EXISTING_SOURCE_ROOTS) \
	$(PYTHON_PRUNED_DIRECTORIES) -type f -name '*.py' -print | sort))
PYTHON_IMPORT_ROOTS = $(addprefix --extra-search-path ,$(PYTHON_EXISTING_SOURCE_ROOTS))
# The CV-005 CodeScene contracts live in shared-actions and run from a full
# commit, so a fix is a pin bump. `.github/cv005.toml` holds this repository's
# only parameters.
CV005_CONTRACTS_REF ?= a38feb9be25755c30eca5bda96bd3786a5b89c6b
CV005_CONTRACTS = $(UV_ENV) $(UV) tool run --managed-python \
	--python $(PYTHON_BASELINE) \
	--from 'git+https://github.com/leynos/shared-actions@$(CV005_CONTRACTS_REF)\#subdirectory=packages/cv005-contracts' \
	cv005-contracts

test-workflow-contracts: test-python ## Check local and shared workflow contracts
	$(CV005_CONTRACTS) check --repository .

build: check-build-tools target/debug/$(TARGET) ## Build debug binary
release: target/release/$(TARGET) ## Build release binary

all: ## Perform a comprehensive check of code
	+$(MAKE) check-fmt
	+$(MAKE) lint
	+$(MAKE) typecheck
	+$(MAKE) test
	+$(MAKE) test-workflow-contracts
	+$(MAKE) spelling

clean: ## Remove build artefacts
	$(CARGO) clean
	rm -f .typos-oxendict-base.json .typos-oxendict-base.toml

test: check-build-tools test-python ## Run tests with warnings treated as errors
	RUSTFLAGS="$${RUSTFLAGS:+$$RUSTFLAGS }$(DEV_RUST_FLAGS)" $(CARGO) $(TEST_CMD) $(TEST_FLAGS) $(BUILD_JOBS)
	RUSTDOCFLAGS="$(RUSTDOC_FLAGS)" RUSTFLAGS="$${RUSTFLAGS:+$$RUSTFLAGS }$(DEV_RUST_FLAGS)" $(CARGO) test --doc --workspace --all-features
ifeq ($(WITH_ACT),1)
	act pull_request --workflows .github/workflows/ci.yml --job build-test --platform ubuntu-latest=catthehacker/ubuntu:act-latest --secret GITHUB_TOKEN --env ACT=true
endif

test-python: ## Run repository-owned Python tests on the baseline interpreter
	$(PYTHON) -m unittest discover -s tests/workflow_contracts -p '*_test.py'

target/debug/$(TARGET): check-build-tools ## Build the debug binary
	RUSTFLAGS="$${RUSTFLAGS:+$$RUSTFLAGS }$(DEV_RUST_FLAGS)" $(CARGO) build $(BUILD_JOBS) --bin $(TARGET)

target/release/$(TARGET): ## Build the release binary
	RUSTFLAGS="$${RUSTFLAGS:+$$RUSTFLAGS }$(RUST_FLAGS) $(POLONIUS_FLAGS)" $(CARGO) build $(BUILD_JOBS) --release --bin $(TARGET)

coverage: check-coverage-tools ## Generate lcov coverage with lld for llvm-tools compatibility
	@echo "coverage linker flags: $(COVERAGE_LINKER_FLAGS)"
	CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER=clang \
		CARGO_PROFILE_DEV_CODEGEN_BACKEND=llvm \
		RUSTFLAGS="$(COVERAGE_RUST_FLAGS)" \
		CFLAGS="$(COVERAGE_LINKER_FLAGS)" \
		LDFLAGS="$(COVERAGE_LINKER_FLAGS)" \
		$(CARGO) llvm-cov --lcov --output-path lcov.info $(TEST_FLAGS)

install-build-tools: ## Install the pinned toolchain and verified build tools
	$(INSTALL_BUILD_TOOLS)
	+$(MAKE) check-build-tools

check-build-tools: ## Verify development build prerequisites
	$(CHECK_BUILD_TOOLS)

check-coverage-tools: ## Verify LLVM coverage prerequisites
	$(CHECK_BUILD_TOOLS) --coverage

lint: check-build-tools ## Run Rust and Python lint gateways sequentially
	+$(MAKE) lint-clippy
	+$(MAKE) lint-whitaker
	+$(MAKE) lint-python

lint-clippy: ## Run rustdoc and Clippy with warnings denied
	RUSTDOCFLAGS="$(RUSTDOC_FLAGS)" RUSTFLAGS="$${RUSTFLAGS:+$$RUSTFLAGS }$(DEV_RUST_FLAGS)" $(CARGO) doc --no-deps
	RUSTFLAGS="$${RUSTFLAGS:+$$RUSTFLAGS }$(DEV_RUST_FLAGS)" $(CARGO) clippy $(CLIPPY_FLAGS)

lint-whitaker: ## Run Whitaker with development Rust flags
	@echo "Whitaker binary: $(WHITAKER)"
	RUSTFLAGS="$${RUSTFLAGS:+$$RUSTFLAGS }$(DEV_RUST_FLAGS)" $(WHITAKER) --all -- $(CARGO_FLAGS)

lint-python: ## Run Ruff, Pylint, df12, authority, and documentation checks
	@if [ -z "$(PYTHON_SOURCES)" ]; then \
		echo "lint-python: no Python sources under $(PYTHON_SOURCE_ROOTS)"; \
	else \
		set -e; \
		$(RUFF) check $(PYTHON_SOURCES); \
		$(PYLINT) $(PYTHON_SOURCES); \
		$(AMBRLEAKS) $(PYTHON_SOURCES); \
		$(INTERROGATE) $(PYTHON_SOURCES); \
	fi

typecheck: check-build-tools typecheck-rust typecheck-python ## Type-check Rust and Python without building

typecheck-rust: check-build-tools ## Type-check Rust without building
	RUSTFLAGS="$${RUSTFLAGS:+$$RUSTFLAGS }$(DEV_RUST_FLAGS)" $(CARGO) check $(CARGO_FLAGS)

typecheck-python: ## Type-check Python sources on the baseline interpreter
	@if [ -z "$(PYTHON_SOURCES)" ]; then \
		echo "typecheck-python: no Python sources under $(PYTHON_SOURCE_ROOTS)"; \
	else \
		$(UV_ENV) $(UV) tool run --managed-python \
			--python $(PYTHON_BASELINE) --from ty==$(TY_VERSION) \
			ty check --python-version $(PYTHON_BASELINE) \
			$(PYTHON_IMPORT_ROOTS) $(PYTHON_SOURCES); \
	fi

fmt: ## Format Rust and Markdown sources
	$(CARGO) fmt --all
	@if [ -n "$(PYTHON_SOURCES)" ]; then $(RUFF) format $(PYTHON_SOURCES); fi
	$(MDTABLEFIX) --in-place $(MDTABLEFIX_SELECT) $(MDTABLEFIX_RULES)
	$(MDLINT) --fix "**/*.md"

check-fmt: ## Verify formatting
	$(CARGO) fmt --all -- --check
	@if [ -n "$(PYTHON_SOURCES)" ]; then $(RUFF) format --check $(PYTHON_SOURCES); fi
	$(MDTABLEFIX) --check $(MDTABLEFIX_SELECT) $(MDTABLEFIX_RULES)

markdownlint: ## Lint Markdown files
	$(MDLINT) '**/*.md'
	+$(MAKE) spelling

spelling: ## Enforce en-GB-oxendict spelling in tracked repository files
	@if ! git rev-parse --is-inside-work-tree >/dev/null 2>&1; then \
		echo "make spelling needs a Git repository: the gate enumerates tracked files with git ls-files. Run: git init && git add -A" >&2; \
		exit 1; \
	fi
	@if [ -z "$$(git ls-files)" ]; then \
		echo "make spelling found no tracked files: the gate enumerates tracked files with git ls-files. Run: git add -A" >&2; \
		exit 1; \
	fi
	$(TYPOS_CONFIG_BUILDER) gate --repository . --scope all

nixie: ## Validate Mermaid diagrams
	$(NIXIE) --no-sandbox

audit: rust-audit ## Audit dependencies for known vulnerabilities

rust-audit: ## Audit the Rust workspace for known vulnerabilities
	set -eo pipefail; \
	manifest_list=$$(mktemp); \
	trap 'rm -f "$$manifest_list"' EXIT; \
	printf "Audit metadata phase: deriving workspace manifests\n"; \
	$(CARGO) metadata --no-deps --format-version 1 | $(PYTHON) -c 'import json, sys; metadata = json.load(sys.stdin); members = set(metadata["workspace_members"]); print(metadata["workspace_root"]); [print(package["manifest_path"]) for package in metadata["packages"] if package["id"] in members]' > "$$manifest_list"; \
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
