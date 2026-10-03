//! Mutations for job-scoped CI build-standard provisioning.

#[path = "build_standard_support/ci_steps.rs"]
mod ci_steps;

use ci_steps::{SETUP_RUST_ACTION_PREFIX, Workflow};

/// A direct Linux route must retain an earlier, unconditional Rust provisioner.
#[test]
fn direct_development_jobs_cannot_bypass_shared_build_provisioning() {
    let ci = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/.github/workflows/ci.yml"
    ));
    let ci_without_linker = ci.replace("          install-mold: 'true'\n", "");
    let discovered_multiline_route =
        "jobs:\n  suite:\n    runs-on: ubuntu-latest\n    steps:\n      - run: |\n          cargo \
         test\n";
    let list_prefix_route =
        "jobs:\n  suite:\n    runs-on: ubuntu-latest\n    steps:\n      - run: cargo test\n";
    let late_setup = format!(
        "jobs:\n  suite:\n    runs-on: ubuntu-latest\n    steps:\n      - run: make test\n{}",
        setup_step("")
    );
    let conditional_setup = format!(
        "jobs:\n  suite:\n    runs-on: ubuntu-latest\n    steps:\n{}",
        setup_step("        if: github.event_name == 'push'\n      - run: cargo check\n")
    );
    let soft_failing_setup = format!(
        "jobs:\n  suite:\n    runs-on: ubuntu-latest\n    steps:\n{}",
        setup_step("        continue-on-error: true\n      - run: cargo doc\n")
    );
    let wrong_action_setup = format!(
        "jobs:\n  suite:\n    runs-on: ubuntu-latest\n    steps:\n{}      - run: cargo test\n",
        setup_step("").replace(SETUP_RUST_ACTION_PREFIX, "owner/setup-rust@unapproved")
    );
    let sibling_setup = format!(
        "jobs:\n  suite:\n    runs-on: ubuntu-latest\n    steps:\n      - run: cargo build\n  \
         helper:\n    runs-on: ubuntu-latest\n    steps:\n{}",
        setup_step("")
    );
    let comment_only_action = format!(
        "jobs:\n  suite:\n    runs-on: ubuntu-latest\n    steps:\n{}",
        setup_step(
            "        # uses: \
             leynos/shared-actions/.github/actions/setup-rust@\
             dependabot-ref\n      - run: cargo test\n"
        )
        .replacen(
            &format!("uses: {SETUP_RUST_ACTION_PREFIX}dependabot-ref"),
            "uses: owner/setup-rust@unapproved",
            1,
        )
    );
    for (name, workflow) in [
        ("ci.yml", ci_without_linker.as_str()),
        ("new-development.yml", discovered_multiline_route),
        ("list-prefix.yml", list_prefix_route),
        ("late-setup.yml", late_setup.as_str()),
        ("conditional-setup.yml", conditional_setup.as_str()),
        ("soft-failing-setup.yml", soft_failing_setup.as_str()),
        ("wrong-action.yml", wrong_action_setup.as_str()),
        ("sibling-setup.yml", sibling_setup.as_str()),
        ("comment-only-action.yml", comment_only_action.as_str()),
    ] {
        let problems = Workflow::new(name, workflow).build_problems();
        assert!(
            !problems.is_empty(),
            "{name} must reject missing, late, conditional, or soft-failing build provision: \
             {problems:#?}"
        );
    }
}

/// Pull-request coverage must not run on push or under `act`.
#[test]
fn ci_coverage_stays_in_the_pull_request_lane() {
    let ci = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/.github/workflows/ci.yml"
    ));
    let (_, coverage) = ci
        .split_once("      - name: Test and Measure Coverage\n")
        .expect("CI must retain its coverage action step");
    let condition = coverage
        .lines()
        .find_map(|line| line.trim().strip_prefix("if: "))
        .unwrap_or("");
    assert!(
        condition.contains("github.event_name == 'pull_request'")
            && condition.contains("env.ACT != 'true'"),
        "CI coverage must be limited to pull requests outside Act: {condition:?}"
    );
}

/// Scope mutations reuse the current CI reader's selected setup and Whitaker checks.
#[test]
fn current_ci_retains_the_shared_build_standard_contract() -> Result<(), String> {
    let ci = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/.github/workflows/ci.yml"
    ));
    let reader = Workflow::new("ci.yml", ci);
    let mut problems = reader.linker_install_problems();
    problems.extend(reader.setup_rust_action_problems());
    problems.extend(reader.whitaker_provisioning_problems());
    problems.extend(ci_steps::workflow_problems()?);
    if problems.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "current CI must retain its shared build-standard contract: {problems:#?}"
        ))
    }
}

/// Linux requires the build provisioner, whereas a macOS-only route does not.
#[test]
fn only_linux_direct_development_routes_require_the_linker() {
    let linux = "jobs:\n  suite:\n    runs-on: [ubuntu-latest, macos-latest]\n    steps:\n      - \
                 run: cargo test\n";
    let macos = "jobs:\n  suite:\n    runs-on:\n      labels: [macos-latest]\n    steps:\n      - \
                 run: cargo test\n";
    let matrix =
        "jobs:\n  suite:\n    runs-on: ${{ matrix.os }}\n    strategy:\n      matrix:\n        \
         os: [macos-latest, ubuntu-latest]\n    steps:\n      - run: cargo test\n";
    let self_hosted_linux =
        "jobs:\n  suite:\n    runs-on:\n      labels: [self-hosted, linux]\n    steps:\n      - \
         run: cargo test\n";
    let unknown =
        "jobs:\n  suite:\n    runs-on: custom-host\n    steps:\n      - run: cargo test\n";
    for (name, workflow, expected) in [
        ("linux.yml", linux, true),
        ("macos.yml", macos, false),
        ("matrix.yml", matrix, true),
        ("self-hosted-linux.yml", self_hosted_linux, true),
        ("unknown.yml", unknown, true),
    ] {
        let problems = Workflow::new(name, workflow).build_problems();
        assert_eq!(
            !problems.is_empty(),
            expected,
            "{name} must apply Linux provisioning only to Linux legs and reject unknown runners: \
             {problems:#?}"
        );
    }
}

/// Builds a selected setup step, followed by the caller-provided route text.
fn setup_step(suffix: &str) -> String {
    format!(
        "      - uses: {SETUP_RUST_ACTION_PREFIX}dependabot-ref\n        with:\n          \
         install-mold: 'true'\n{suffix}"
    )
}

/// Every coverage action step must retain the backend and execution contract.
#[test]
fn later_coverage_steps_cannot_bypass_backend_or_execution_contracts() {
    let ci = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/.github/workflows/ci.yml"
    ));
    let (_, first_coverage) = ci
        .split_once("      - name: Test and Measure Coverage\n")
        .expect("CI must retain the coverage step used as a valid first fixture");
    let second_coverage = format!("      - name: Test and Measure Coverage\n{first_coverage}");
    let missing_backend = second_coverage.replace(
        "CARGO_PROFILE_DEV_CODEGEN_BACKEND: llvm",
        "CARGO_PROFILE_DEV_CODEGEN_BACKEND: cranelift",
    );
    let missing_execution_field = second_coverage.replacen("          format: lcov\n", "", 1);
    let backend_problems = Workflow::new("fixture.yml", &format!("{ci}\n{missing_backend}"))
        .coverage_backend_problems();
    let execution_problems =
        Workflow::new("fixture.yml", &format!("{ci}\n{missing_execution_field}"))
            .coverage_execution_problems();
    assert_eq!(
        backend_problems.len(),
        1,
        "the second coverage action without LLVM must be rejected: {backend_problems:#?}"
    );
    assert_eq!(
        execution_problems.len(),
        1,
        "the second coverage action without its parity field must be rejected: \
         {execution_problems:#?}"
    );
}

/// Corpus discovery must apply every coverage requirement to an added action.
#[test]
fn discovered_coverage_actions_cannot_bypass_the_selected_contract() {
    let workflow = format!(
        concat!(
            "jobs:\n  coverage:\n    runs-on: ubuntu-latest\n    steps:\n{}",
            "      - uses: owner/shared-actions/.github/actions/generate-coverage@unapproved\n",
            "        env:\n          CARGO_PROFILE_DEV_CODEGEN_BACKEND: cranelift\n",
            "        with:\n          language: rust\n",
        ),
        setup_step("")
    );
    let problems = Workflow::new("added-coverage.yml", &workflow).build_problems();
    assert!(
        problems
            .iter()
            .any(|problem| problem.contains("coverage must retain uses:"))
            && problems
                .iter()
                .any(|problem| problem.contains("CODEGEN_BACKEND to llvm")),
        "an added coverage action without the selected revision/backend/inputs must fail: \
         {problems:#?}"
    );
}
