# User Guide

This guide explains how to use the generated Mornington project after rendering
it from the template.

## First Steps After Rendering

Initialize version control and stage the rendered files before running any gate:

```sh
git init
git add -A
make all
```

Each gate selects its Markdown differently. `make check-fmt` runs
`mdtablefix --check --git --include-untracked`, which reads the tracked files
and the untracked files Git does not ignore, so it needs a Git repository but
not staged files. `make markdownlint` runs `markdownlint-cli2 '**/*.md'` over
every matching file, staged or not, and then runs `make spelling`. The spelling
gate enumerates its inputs with `git ls-files`, so it checks only files Git
already tracks: stage new Markdown before relying on it.

## Generated Tooling

Generated projects use Rust 2024, a pinned nightly toolchain, strict lint
settings, and documented starter code. Library projects render `src/lib.rs`.
Application projects render `src/main.rs`, `src/lib.rs`, release automation, and
`[package.metadata.binstall]` metadata for binary installation.

This project enables the Polonius alpha borrow checker (`-Zpolonius=next`) on
its dated nightly toolchain. See the [Polonius policy](polonius.md) before
changing the toolchain or borrow-centric APIs.

## Rust Build Standard

Development builds use nightly `-Zthreads=8` for the parallel rustc frontend
and Cranelift for debug code generation. On Linux, the checked-in Cargo
configuration adds `-C link-arg=-fuse-ld=mold`; other platforms omit this
linker selection. Before building on a fresh Linux checkout, run
`make install-build-tools` to install the pinned toolchain and
checksum-verified mold binary, then run `make check-build-tools` to verify the
toolchain, components, clang, and mold.

Cargo selects one `rustflags` source rather than merging the configured sources
with environment flags. Setting `RUSTFLAGS` replaces the values from
`.cargo/config.toml`. The Makefile's development Cargo build, test, typecheck,
Clippy, and Rustdoc commands preserve the caller's `RUSTFLAGS` and append the
required warning denial, Polonius, nightly frontend, and Linux linker flags.
The Whitaker lint invocation also preserves caller flags and appends the
project development flags to its own Cargo work. Use the Make targets when
invoking development builds so caller-specific flags do not discard the project
defaults.

Coverage and release are deliberate exceptions to the development flags.
Coverage sets `CARGO_PROFILE_DEV_CODEGEN_BACKEND=llvm` to choose the dev
code-generation backend for instrumentation. Separately, it selects `clang`
with the `lld` linker (`-fuse-ld=lld`) for LLVM coverage compatibility; `lld`
is a linker, not a code-generation backend. Coverage omits `-Zthreads=8` and
the development linker selection, and `make check-coverage-tools` also verifies
`ld.lld` is available. Release preserves caller flags, warning denial, and
Polonius while omitting the development-only frontend and linker flags.

## Validation and Environment Policy

This project denies `unknown_lints`, `renamed_and_removed_lints`,
`unsafe_code`, and `missing_docs`. Rustdoc denies `missing_crate_level_docs`,
`broken_intra_doc_links`, `private_intra_doc_links`, `bare_urls`,
`invalid_html_tags`, `invalid_codeblock_attributes`, and `unescaped_backticks`.
Clippy denies `missing_assert_message` and `missing_docs_in_private_items`, and
uses `disallowed_methods` to reject direct calls to process-environment
readers, iterators, and mutation functions. Warnings are validation failures;
these policies are not advisory.

Code that depends on environment variables must receive `mockable::Env` (or a
narrow equivalent closure) as a dependency. The production composition root
constructs `mockable::DefaultEnv`, while tests use `mockable::MockEnv`.
In-process tests must not mutate the process environment or serialize mutation
behind `Mutex`, `OnceLock`, or `serial_test`. The only mutation exception is an
end-to-end test using `assert_cmd`: `Command::env` and `Command::env_clear`
configure the isolated child process, not the test harness.

`make lint` builds documentation with Rustdoc warnings denied before running
Clippy and Whitaker. `make test` exports warning-denial flags to the selected
test runner and separately runs all-feature workspace doctests with Rustdoc
warnings denied. A warning from normal tests, the documentation build, or a
doctest therefore fails the command.

### Migrate an Existing Project

1. Copy the current `[lints.clippy]`, `[lints.rust]`, and `[lints.rustdoc]`
   policy into each package manifest, or opt every workspace member into
   equivalent workspace lints.
2. Add the current `disallowed-methods` entries to the workspace-root
   `clippy.toml`.
3. Update the Makefile so `make lint` passes mandatory `RUSTDOCFLAGS` to
   `cargo doc` and `make test` passes them to workspace doctests. Preserve
   mandatory flags when appending inherited flags.
4. Declare the current `mockable` dependency, refactor direct `std::env` reads
   behind an injected `mockable::Env`, and replace in-process environment
   mutation with `mockable::MockEnv`. Wire the environment adapter at the
   composition root and construct `mockable::DefaultEnv` only in production.
5. Update contributor guidance in `AGENTS.md` to document the injection
   boundary and the `assert_cmd` child-process exception.
6. Add diagnostic messages to assertions and crate/module/public-item
   documentation where the denied lints require it.
7. Run `make check-fmt`, `make lint`, `make typecheck`, and `make test`. Fix
   every warning rather than suppressing or downgrading it.

## Makefile Targets

The generated `Makefile` exposes these public targets:

- `make all` runs formatting checks, linting, tests, and spelling checks.
- `make check-fmt` verifies Rust and Python formatting and, with
  `mdtablefix --check --git --include-untracked`, Markdown formatting.
- `make fmt` formats Rust, Python, and Markdown sources.
- `make lint` builds documentation, runs Clippy and Whitaker, then runs Ruff,
  Pylint with the df12 policy, ambrleaks, and Interrogate on repository-owned
  Python. Every warning is denied.
- `make typecheck` checks Rust and runs ty over repository-owned Python without
  building the application.
- `make test` runs the Python workflow contracts, then runs
  `cargo nextest run` when cargo-nextest is installed and falls back to
  `cargo test` otherwise. It denies warnings in normal tests and in the
  separate all-feature workspace doctest run.
- `make build` builds the debug target.
- `make release` builds the release target.
- `make coverage` writes `lcov.info` using `cargo llvm-cov` and `lld`.
- `make install-build-tools` installs the pinned Rust toolchain and
  checksum-verified mold binary, then checks them.
- `make check-build-tools` verifies the normal development prerequisites;
  `make check-coverage-tools` also requires `ld.lld`.
- `make audit` derives the Rust workspace root with `cargo metadata` and runs
  `cargo audit` once from that root. In PR CI, Dependabot runs skip
  `make audit` and the audit-only setup when `github.actor` is
  `dependabot[bot]`, so whole-lockfile advisories do not block unrelated
  dependency updates. Human PRs still retain the audit gate, and
  `.github/workflows/audit.yml` runs weekly and can also be triggered manually
  as the compensating control.
- `make test-workflow-contracts` runs the shared CV-005 CodeScene workflow
  contract over the checked-in workflows. Its repository-specific pairing is in
  `.github/cv005.toml`; the target needs `uv`, which fetches the managed Python
  3.14 baseline.
- `make markdownlint` checks Markdown files and enforces en-GB-oxendict
  spelling.
- `make spelling` runs the shared `typos-config-builder` v0.1.3 gate. It
  regenerates `typos.toml` from the live shared dictionary and the
  `typos.local.toml` overlay, then checks Markdown prose, so `typos.toml` is
  never drift checked in CI. Commit its regenerated output with spelling-policy
  changes, but never edit it by hand. Keep repository-specific exceptions in
  the tracked `typos.local.toml` overlay. The gate enumerates its inputs with
  `git ls-files`, so the project must be a Git repository in which Git already
  tracks some files; `make spelling` fails with that instruction when
  `git ls-files` lists nothing, and checks only the files Git tracks.
- `make nixie` validates Mermaid diagrams.

Install `clang`, `lld`, `uv`, and `cargo-audit`, then run
`make install-build-tools`, before running the full generated workflow locally
on Linux.

## Scheduled Mutation Testing

Generated projects include `.github/workflows/mutation-testing.yml`, a
scheduled GitHub Actions workflow that runs mutation testing with
`cargo-mutants`. It is a thin caller of the shared `leynos/shared-actions`
`mutation-cargo` reusable workflow.

Mutation testing measures test-suite quality. It introduces small changes
(mutants) into the source and confirms the tests fail in response. A surviving
mutant marks a code path the tests do not meaningfully exercise, so promote it
into a new test rather than ignoring it.

The workflow runs on a daily schedule (09:15 UTC by default) and can also be
started manually from the **Actions** tab with **Run workflow**. Scheduled runs
mutate only files changed within the detection window, so routine runs stay
fast; a manual dispatch mutates the whole crate, fanned out across shards.
Because the runs are scheduled rather than gating pull requests, they surface
coverage gaps without slowing day-to-day CI, and they do not block merges.

When adopting the workflow in a new repository, stagger the cron slot: pick an
unclaimed daily time to avoid concurrent runs across related repositories. The
`mutation` job runs with a least-privilege token (`contents: read` plus
`id-token: write` for workflow-source resolution).

Results land in the run's job summary: a final job posts per-target outcome
counts and a table of surviving mutants, and each shard uploads its
`mutants.out/` directory as a `mutation-report-*` artefact. When nothing
relevant changed, the run writes a skip message and finishes in seconds.
Surviving mutants and timeouts are informational and leave the run green, so a
red run means something actually broke — a usage error, an already-failing test
baseline, or an internal error. Watch for those through GitHub's notifications
for failed scheduled runs.
