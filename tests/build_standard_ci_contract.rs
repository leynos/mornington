//! Contracts for CI's coverage backend and strict Whitaker provisioning.

#[path = "build_standard_support/ci_steps.rs"]
mod ci_steps;

use ci_steps::{SETUP_RUST_ACTION_PREFIX, Workflow, workflow_problems};
use rstest::rstest;

/// The live pull-request workflow under the same fixture contracts.
const CI_WORKFLOW: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/.github/workflows/ci.yml"
));

/// A workflow step that passes the input, quoted.
const STEP_INSTALLS: &str = concat!(
    "    steps:\n      - name: Setup Rust\n",
    "        uses: leynos/shared-actions/.github/actions/setup-rust@dependabot-ref-one\n",
    "        with:\n          install-mold: 'true'\n"
);
/// The same, with the bare value.
const STEP_INSTALLS_BARE: &str = concat!(
    "    steps:\n      - uses: \
     leynos/shared-actions/.github/actions/setup-rust@dependabot-ref-one\n",
    "        with:\n          install-mold: true\n"
);
/// A step with no input at all.
const STEP_MISSING_INPUT: &str = concat!(
    "    steps:\n      - name: Setup Rust\n",
    "        uses: leynos/shared-actions/.github/actions/setup-rust@dependabot-ref-one\n"
);
/// A step that turns the input off.
const STEP_INPUT_OFF: &str = concat!(
    "    steps:\n      - name: Setup Rust\n",
    "        uses: leynos/shared-actions/.github/actions/setup-rust@dependabot-ref-one\n",
    "        with:\n          install-mold: 'false'\n"
);
/// A step without the input, followed by a step that has one for another action.
const STEP_BEFORE_A_SIBLING_THAT_INSTALLS: &str = concat!(
    "    steps:\n      - name: Setup Rust\n",
    "        uses: leynos/shared-actions/.github/actions/setup-rust@dependabot-ref-one\n",
    "      - name: Other\n        uses: org/other@abc\n        with:\n          install-mold: \
     'true'\n"
);
/// A comment that names the action, and no step.
const COMMENT_NAMING_THE_ACTION: &str =
    "    steps:\n      # setup-rust@abc installs it\n      - run: make\n";

/// A coverage step that selects the instrumentation-compatible backend.
const COVERAGE_WITH_LLVM: &str = concat!(
    "    steps:\n      - uses: \
     org/shared-actions/.github/actions/generate-coverage@0123456789abcdef\n",
    "        env:\n          CARGO_PROFILE_DEV_CODEGEN_BACKEND: llvm\n"
);
/// A coverage step that leaves the development backend to Cargo configuration.
const COVERAGE_WITHOUT_LLVM: &str = concat!(
    "    steps:\n      - uses: \
     org/shared-actions/.github/actions/generate-coverage@0123456789abcdef\n",
    "        env:\n          RUSTFLAGS: -C link-arg=-fuse-ld=lld\n"
);

/// A coverage step that assigns `RUSTFLAGS` without a development flag.
const COVERAGE_WITH_NEUTRAL_RUSTFLAGS: &str = concat!(
    "    steps:\n      - name: Cover\n",
    "        uses: org/shared-actions/.github/actions/generate-coverage@",
    "0123456789abcdef0123456789abcdef01234567\n",
    "        env:\n          RUSTFLAGS: -D warnings\n"
);
/// A coverage step whose `RUSTFLAGS` omits the required explicit assignment.
const COVERAGE_WITHOUT_RUSTFLAGS: &str = concat!(
    "    steps:\n      - name: Cover\n",
    "        uses: org/shared-actions/.github/actions/generate-coverage@",
    "0123456789abcdef0123456789abcdef01234567\n"
);
/// A coverage step whose neighbouring step owns the only `RUSTFLAGS` field.
const COVERAGE_WITH_SIBLING_RUSTFLAGS: &str = concat!(
    "    steps:\n      - name: Cover\n",
    "        uses: org/shared-actions/.github/actions/generate-coverage@",
    "0123456789abcdef0123456789abcdef01234567\n",
    "      - name: Other\n        env:\n          RUSTFLAGS: -D warnings\n"
);
/// A coverage step that inherits the development frontend flag.
const COVERAGE_WITH_THREADS: &str = concat!(
    "    steps:\n      - name: Cover\n",
    "        uses: org/shared-actions/.github/actions/generate-coverage@",
    "0123456789abcdef0123456789abcdef01234567\n",
    "        env:\n          RUSTFLAGS: -D warnings -Zthreads=8\n"
);
/// A coverage step that inherits the development linker flag.
const COVERAGE_WITH_LINKER: &str = concat!(
    "    steps:\n      - name: Cover\n",
    "        uses: org/shared-actions/.github/actions/generate-coverage@",
    "0123456789abcdef0123456789abcdef01234567\n",
    "        env:\n          RUSTFLAGS: -Clink-arg=-fuse-ld=mold\n"
);

/// A CI Whitaker step that uses the audited strict provisioner.
const WHITAKER_WITH_APPROVED_ACTION: &str = concat!(
    "    steps:\n      - name: Install Whitaker\n",
    "        uses: leynos/shared-actions/.github/actions/install-whitaker@",
    "6dea5677a84fec60ca51b07202570e3af12ffdb4\n",
    "        with:\n          cranelift: 'true'\n",
    "      - name: Lint\n        run: make lint\n"
);
/// A CI Whitaker step that uses a rolling ref and a prohibited suite override.
const WHITAKER_WITH_UNAPPROVED_ACTION: &str = concat!(
    "    steps:\n      - name: Install Whitaker\n",
    "        uses: leynos/shared-actions/.github/actions/install-whitaker@main\n",
    "        with:\n          suite-version: rolling\n"
);

/// Scenario: coverage action steps with and without an explicit LLVM backend.
///
/// Invariant: coverage selects LLVM itself, because the development profile
/// otherwise selects Cranelift for ordinary builds.
#[rstest]
#[case::llvm_backend(COVERAGE_WITH_LLVM, 0)]
#[case::missing_backend(COVERAGE_WITHOUT_LLVM, 1)]
#[case::live_ci(CI_WORKFLOW, 0)]
fn coverage_steps_select_the_llvm_backend(#[case] workflow: &str, #[case] expected: usize) {
    let found = Workflow::new("fixture.yml", workflow)
        .coverage_backend_problems()
        .len();
    assert_eq!(
        found, expected,
        "coverage backend reader found {found} complaints, not {expected}"
    );
}

/// Coverage must assign its own neutral `RUSTFLAGS`, rather than inheriting a
/// development frontend or linker setting from another route.
#[rstest]
#[case::neutral(COVERAGE_WITH_NEUTRAL_RUSTFLAGS, 0)]
#[case::unassigned(COVERAGE_WITHOUT_RUSTFLAGS, 1)]
#[case::sibling_assignment(COVERAGE_WITH_SIBLING_RUSTFLAGS, 1)]
#[case::frontend(COVERAGE_WITH_THREADS, 1)]
#[case::linker(COVERAGE_WITH_LINKER, 1)]
fn coverage_steps_keep_rustflags_out_of_the_development_standard(
    #[case] workflow: &str,
    #[case] expected: usize,
) {
    let found = Workflow::new("fixture.yml", workflow)
        .coverage_problems()
        .len();
    assert_eq!(
        found, expected,
        "coverage RUSTFLAGS reader found {found} complaints, not {expected}"
    );
}

/// Scenario: CI's strict Whitaker action and a rolling alternative.
///
/// Invariant: this repository uses the reviewed shared provisioner at its
/// allowlisted revision, without direct installation or suite overrides.
#[rstest]
#[case::approved(WHITAKER_WITH_APPROVED_ACTION, 0)]
#[case::unapproved(WHITAKER_WITH_UNAPPROVED_ACTION, 2)]
#[case::live_ci(CI_WORKFLOW, 0)]
fn whitaker_provisioning_is_strict(#[case] workflow: &str, #[case] expected: usize) {
    let found = Workflow::new("fixture.yml", workflow)
        .whitaker_provisioning_problems()
        .len();
    assert_eq!(
        found, expected,
        "Whitaker provisioner found {found} complaints, not {expected}"
    );
}

/// The strict installer must configure Cranelift and complete before lint; a
/// conditional or soft-failing provision leaves a reachable lint route without
/// Whitaker.
#[rstest]
#[case::missing_cranelift(
    WHITAKER_WITH_APPROVED_ACTION.replace("cranelift: 'true'", "cranelift: 'false'")
)]
#[case::conditional(
    WHITAKER_WITH_APPROVED_ACTION.replace(
        "        with:\n",
        "        if: github.event_name == 'push'\n        with:\n"
    )
)]
#[case::soft_failing(
    WHITAKER_WITH_APPROVED_ACTION.replace(
        "        with:\n",
        "        continue-on-error: true\n        with:\n"
    )
)]
fn strict_whitaker_installer_cannot_be_weakened(#[case] workflow: String) {
    let found = Workflow::new("fixture.yml", &workflow)
        .whitaker_provisioning_problems()
        .len();
    assert_eq!(
        found, 1,
        "a weakened strict Whitaker provisioner must produce one complaint"
    );
}

/// Moving the installer after lint makes the lint route unreachable through
/// the selected Whitaker contract.
#[test]
fn strict_whitaker_installer_precedes_lint() {
    let workflow = concat!(
        "    steps:\n      - name: Lint\n        run: make lint\n",
        "      - name: Install Whitaker\n",
        "        uses: leynos/shared-actions/.github/actions/install-whitaker@",
        "6dea5677a84fec60ca51b07202570e3af12ffdb4\n",
        "        with:\n          cranelift: 'true'\n"
    );
    let found = Workflow::new("fixture.yml", workflow)
        .whitaker_provisioning_problems()
        .len();
    assert_eq!(
        found, 1,
        "moving Whitaker installation after lint must be rejected"
    );
}

/// Pull-request and main coverage share every execution field except publishing.
#[test]
fn coverage_lanes_share_the_selected_execution_contract() {
    for (name, workflow) in [
        (
            "ci.yml",
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/.github/workflows/ci.yml"
            )),
        ),
        (
            "coverage-main.yml",
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/.github/workflows/coverage-main.yml"
            )),
        ),
    ] {
        let problems = Workflow::new(name, workflow).coverage_execution_problems();
        assert!(
            problems.is_empty(),
            "{name} must retain the shared coverage execution contract: {problems:#?}"
        );
    }
    let ci = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/.github/workflows/ci.yml"
    ));
    assert!(
        ci.contains("publish-artefact: 'false'"),
        "the pull-request lane must not publish a coverage artefact"
    );
    assert!(
        !ci.contains("publish-baseline:"),
        "the pull-request lane must not write the main coverage baseline"
    );
    assert!(
        !ci.contains("codescene"),
        "the pull-request lane must not include the CodeScene publisher"
    );
}

/// The listed build workflows retain their setup-rust and mold contract.
#[test]
fn ci_workflow_list_remains_compliant() -> Result<(), String> {
    let problems = workflow_problems()?;
    if problems.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "listed build workflows must retain their setup-rust contracts: {problems:#?}"
        ))
    }
}

/// Scenario: workflow steps that set up Rust with and without the input.
///
/// Invariant: a step must pass `install-mold: 'true'` itself; another step's
/// input does not count, and a comment naming the action is not a step.
#[rstest]
#[case::quoted_true(STEP_INSTALLS, 0)]
#[case::bare_true(STEP_INSTALLS_BARE, 0)]
#[case::missing_input(STEP_MISSING_INPUT, 1)]
#[case::input_off(STEP_INPUT_OFF, 1)]
#[case::input_on_a_sibling_step(STEP_BEFORE_A_SIBLING_THAT_INSTALLS, 1)]
#[case::comment_only(COMMENT_NAMING_THE_ACTION, 0)]
fn the_workflow_reader_wants_the_input_on_each_step(
    #[case] workflow: &str,
    #[case] expected: usize,
) {
    let found = Workflow::new("fixture.yml", workflow)
        .linker_install_problems()
        .len();
    assert_eq!(
        found, expected,
        "{workflow:?}: the workflow reader found {found} problems, not {expected}"
    );
}

/// A listed workflow must have a setup-rust step, and every such step installs
/// mold so Linux jobs can use the configured linker.
#[test]
fn every_setup_rust_step_installs_linker() -> Result<(), String> {
    let problems = workflow_problems()?;
    if problems.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "every setup-rust step must install mold: {problems:#?}"
        ))
    }
}

/// Scenario: setup-rust refs remain owned by Dependabot.
///
/// Invariant: any non-empty revision of the shared action is accepted, while
/// every setup step must continue to pass the mold installer input.
#[test]
fn setup_rust_reference_is_dependabot_managed_and_installs_linker() -> Result<(), String> {
    let alternate_ref = STEP_INSTALLS.replace("dependabot-ref-one", "dependabot-ref-two");
    if alternate_ref == STEP_INSTALLS {
        return Err("the alternate setup-rust fixture ref was not applied".to_owned());
    }
    let reader = Workflow::new("fixture.yml", &alternate_ref);
    let action_problems = reader.setup_rust_action_problems();
    if !action_problems.is_empty() {
        return Err(format!(
            "a Dependabot-managed shared action ref was rejected: {action_problems:#?}"
        ));
    }
    if !reader.linker_install_problems().is_empty() {
        return Err("a valid shared action ref must retain install-mold".to_owned());
    }
    let empty_ref = alternate_ref.replace("dependabot-ref-two", "");
    let empty_ref_problems = Workflow::new("fixture.yml", &empty_ref).setup_rust_action_problems();
    if empty_ref_problems.len() != 1 {
        return Err(format!(
            "a setup-rust action without a ref must be rejected: {empty_ref_problems:#?}"
        ));
    }

    let wrong_action = alternate_ref.replace(
        SETUP_RUST_ACTION_PREFIX,
        "other/shared-actions/.github/actions/setup-rust@",
    );
    let wrong_action_problems =
        Workflow::new("fixture.yml", &wrong_action).setup_rust_action_problems();
    if wrong_action_problems.len() != 1 {
        return Err(format!(
            "a setup-rust action outside the shared repository must be rejected: \
             {wrong_action_problems:#?}"
        ));
    }

    let without_linker = alternate_ref.replace("install-mold: 'true'\n", "");
    let linker_problems = Workflow::new("fixture.yml", &without_linker).linker_install_problems();
    if linker_problems.len() == 1 {
        Ok(())
    } else {
        Err(format!(
            "removing install-mold must remain a contract violation: {linker_problems:#?}"
        ))
    }
}
