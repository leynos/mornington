//! Contracts for every discovered Linux workflow route that can execute tests.

#[path = "build_standard_support/ci_steps.rs"]
mod ci_steps;
#[path = "build_standard_support/workflow_routes.rs"]
mod workflow_routes;

use ci_steps::{Workflow, workflow_corpus, workflow_problems};
use rstest::rstest;
use workflow_routes::suite_provisioning_problems;

const SHARED_SETUP: &str = concat!(
    "      - uses: leynos/shared-actions/.github/actions/setup-rust@",
    "dependabot-ref\n",
    "        with:\n          install-mold: 'true'\n"
);

fn route_problems(workflows: &[(&str, &str)]) -> Vec<String> {
    suite_provisioning_problems(workflows)
}

/// Checks one local reusable caller together with its supplied child workflow.
fn local_reusable_route_problems(child: &str) -> Vec<String> {
    let caller = "jobs:\n  caller:\n    uses: ./.github/workflows/child.yml\n";
    route_problems(&[("caller.yml", caller), ("child.yml", child)])
}

/// Scalar, inline-list, mapping, and matrix runners all expose a Linux suite
/// and must see the shared installer before it.
#[rstest]
#[case::scalar("runs-on: ubuntu-latest")]
#[case::list("runs-on: [ubuntu-latest, linux]")]
#[case::self_hosted_x64("runs-on: [self-hosted, linux, x64]")]
#[case::mapping(
    "runs-on:\n      labels:\n        - self-hosted\n        - linux\n        - x64\n        - \
     build-fleet"
)]
#[case::group_and_labels(
    "runs-on:\n      group: build-fleet\n      labels: [self-hosted, linux, x64]"
)]
#[case::matrix(
    "runs-on: ${{ matrix.os }}\n    strategy:\n      matrix:\n        os: [ubuntu-latest, \
     macos-latest]"
)]
fn linux_runner_forms_require_a_prior_shared_provision(#[case] runner: &str) {
    let workflow = format!(
        "jobs:\n  suite:\n    {runner}\n    steps:\n{SHARED_SETUP}      - run: make test\n"
    );
    let problems = route_problems(&[("fixture.yml", &workflow)]);
    assert!(
        problems.is_empty(),
        "the supported Linux runner form must retain the prior provision: {problems:#?}"
    );
}

/// Self-hosted metadata without an explicit OS cannot establish a safe route.
#[test]
fn self_hosted_labels_without_an_os_fail_closed() {
    let workflow = format!(
        "jobs:\n  suite:\n    runs-on: [self-hosted, x64, build-fleet]\n    \
         steps:\n{SHARED_SETUP}      - run: make test\n"
    );
    let problems = route_problems(&[("fixture.yml", &workflow)]);
    assert_eq!(
        problems.len(),
        1,
        "self-hosted labels without an operating-system label must fail closed: {problems:#?}"
    );
}

/// The shared action must include a ref, but its value is managed by Dependabot.
#[test]
fn shared_setup_requires_a_nonempty_dependabot_managed_ref() {
    let setup_without_ref = SHARED_SETUP.replace("dependabot-ref", "");
    let workflow = format!(
        "jobs:\n  suite:\n    runs-on: ubuntu-latest\n    steps:\n{setup_without_ref}      - run: \
         make test\n"
    );
    let problems = route_problems(&[("empty-ref.yml", &workflow)]);
    assert_eq!(
        problems.len(),
        1,
        "a shared setup-rust action without a ref must be rejected: {problems:#?}"
    );
}

/// Contradictory runner labels cannot make an ambiguous route appear Linux.
#[rstest]
#[case::conflicting_labels("runs-on: [self-hosted, linux, windows]")]
#[case::contradictory_token("runs-on: [self-hosted, linux, windows-linux, x64]")]
fn conflicting_runner_labels_fail_closed(#[case] runner: &str) {
    let workflow = format!(
        "jobs:\n  suite:\n    {runner}\n    steps:\n{SHARED_SETUP}      - run: make test\n"
    );
    let problems = route_problems(&[("fixture.yml", &workflow)]);
    assert_eq!(
        problems.len(),
        1,
        "contradictory runner operating-system labels must fail closed: {problems:#?}"
    );
}

/// A local reusable caller remains reachable through its checked-in child.
#[test]
fn local_reusable_workflows_are_followed() {
    let child = format!(
        "on: workflow_call\njobs:\n  child-suite:\n    runs-on: ubuntu-latest\n    \
         steps:\n{SHARED_SETUP}      - run: cargo test --all-features\n"
    );
    let problems = local_reusable_route_problems(&child);
    assert!(
        problems.is_empty(),
        "the local reusable suite must be checked through its child: {problems:#?}"
    );
}

/// A broken child route is reported once even when both it and its caller are
/// discovered as corpus entrypoints.
#[test]
fn local_reusable_workflow_failures_are_reported_once() {
    let child = concat!(
        "on: workflow_call\n",
        "jobs:\n",
        "  child-suite:\n",
        "    runs-on: ubuntu-latest\n",
        "    steps:\n",
        "      - run: cargo test --all-features\n"
    );
    let problems = local_reusable_route_problems(child);
    assert_eq!(
        problems.len(),
        1,
        "a broken reusable child route must have one stable diagnostic: {problems:#?}"
    );
}

/// A local reusable call cycle has no finite provision path and is refused.
#[test]
fn local_reusable_workflow_cycles_fail_closed() {
    let first = "jobs:\n  first:\n    uses: ./.github/workflows/second.yml\n";
    let second = "jobs:\n  second:\n    uses: ./.github/workflows/first.yml\n";
    let problems = route_problems(&[("first.yml", first), ("second.yml", second)]);
    assert_eq!(
        problems.len(),
        2,
        "each discovered entrypoint into a reusable-workflow cycle must fail closed: {problems:#?}"
    );
}

/// Adding a new Linux test job cannot avoid the corpus contract merely because
/// it has no current job-name allowlist entry.
#[test]
fn a_new_reachable_linux_suite_without_provisioning_fails() {
    let workflow =
        "jobs:\n  added-suite:\n    runs-on: ubuntu-latest\n    steps:\n      - run: make test\n";
    let problems = route_problems(&[("new.yml", workflow)]);
    assert_eq!(
        problems.len(),
        1,
        "a newly added Linux suite must be rejected when it has no prior provision: {problems:#?}"
    );
}

/// The provision is useful only before the test and when Actions cannot skip
/// or ignore it.
#[rstest]
#[case::after_suite("after")]
#[case::conditional("conditional")]
#[case::continue_on_error("continue")]
fn moved_or_hidden_provisioning_fails(#[case] mutation: &str) -> Result<(), String> {
    let steps = match mutation {
        "after" => format!("      - run: make test\n{SHARED_SETUP}"),
        "conditional" => format!(
            "{SHARED_SETUP}        if: github.event_name == 'push'\n      - run: make test\n"
        ),
        "continue" => {
            format!("{SHARED_SETUP}        continue-on-error: true\n      - run: make test\n")
        }
        _ => return Err(format!("unsupported provisioning mutation: {mutation}")),
    };
    let workflow = format!("jobs:\n  suite:\n    runs-on: ubuntu-latest\n    steps:\n{steps}");
    let problems = route_problems(&[("fixture.yml", &workflow)]);
    if problems.len() == 1 {
        Ok(())
    } else {
        Err(format!(
            "a moved, conditional, or ignored provision must reject the route: {problems:#?}"
        ))
    }
}

/// The repository corpus is discovered at test time and cannot become empty.
#[test]
fn every_checked_in_workflow_is_in_the_routing_corpus() -> Result<(), String> {
    let corpus = workflow_corpus()?;
    let workflows: Vec<(&str, &str)> = corpus
        .iter()
        .map(|(name, workflow)| (name.as_str(), workflow.as_str()))
        .collect();
    let mut problems = route_problems(&workflows);
    problems.extend(workflow_problems()?);
    for (name, text) in &corpus {
        let workflow = Workflow::new(name, text);
        if text.contains("generate-coverage@") {
            problems.extend(workflow.coverage_backend_problems());
            problems.extend(workflow.coverage_execution_problems());
        }
        if text.contains("run: make lint") {
            problems.extend(workflow.whitaker_provisioning_problems());
        }
    }
    let ci = Workflow::new(
        "ci.yml",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/.github/workflows/ci.yml"
        )),
    );
    problems.extend(ci.linker_install_problems());
    problems.extend(ci.setup_rust_action_problems());
    if problems.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "every discovered Linux route must provision build tools and retain its CI build \
             contract: {problems:#?}"
        ))
    }
}
