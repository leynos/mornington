//! Regression contracts for the generated Act workflow's Linux linker setup.

fn workflow_step<'workflow>(workflow: &'workflow str, name: &str) -> (&'workflow str, usize) {
    let marker = format!("- name: {name}\n");
    let Some((preceding_workflow, remaining_workflow)) = workflow.split_once(&marker) else {
        panic!("Act validation must retain the required workflow step: {name}");
    };
    let Some(step) = remaining_workflow.split("\n      - name: ").next() else {
        panic!("a named GitHub Actions workflow step must contain a body: {name}");
    };

    (step, preceding_workflow.len())
}

fn make_conditional_body<'makefile>(makefile: &'makefile str, condition: &str) -> &'makefile str {
    let marker = format!("ifeq ({condition})\n");
    let Some((_, remaining_makefile)) = makefile.split_once(&marker) else {
        panic!("the Makefile must retain the required conditional: {condition}");
    };
    let Some(body) = remaining_makefile
        .split_once("\nendif")
        .map(|(body, _)| body)
    else {
        panic!("the Makefile conditional must terminate with endif: {condition}");
    };

    body
}

/// The outer Cargo test process links binaries before any nested Act execution.
#[test]
fn act_validation_installs_the_configured_linker_before_tests() {
    let workflow = include_str!("../.github/workflows/act-validation.yml");
    let (linker_step, linker_step_offset) =
        workflow_step(workflow, "Install Linux linker prerequisites");
    let (_, test_step_offset) = workflow_step(workflow, "Run tests with act validation");
    let install_command = linker_step
        .lines()
        .map(str::trim)
        .find(|line| line.starts_with("sudo apt-get install "))
        .expect("linker setup must execute sudo apt-get install");

    assert!(
        install_command
            .split_whitespace()
            .any(|word| word == "clang"),
        "the executable apt-get install command must install clang: {install_command}"
    );
    assert!(
        install_command
            .split_whitespace()
            .any(|word| word == "mold"),
        "the executable apt-get install command must install mold: {install_command}"
    );
    assert!(
        linker_step_offset < test_step_offset,
        "Act validation must install linker prerequisites before Cargo links test binaries"
    );
}

/// The runner probes both installed linkers before Cargo invokes nested Act checks.
#[test]
fn act_validation_verifies_linkers_before_running_tests() {
    let workflow = include_str!("../.github/workflows/act-validation.yml");
    let (linker_verification_step, linker_verification_step_offset) =
        workflow_step(workflow, "Verify Linux linker prerequisites");
    let (_, act_installation_step_offset) = workflow_step(workflow, "Install act");
    let (test_step, test_step_offset) = workflow_step(workflow, "Run tests with act validation");

    assert!(
        linker_verification_step
            .lines()
            .map(str::trim)
            .any(|line| line == "clang --version"),
        "Act validation must invoke clang on the outer Linux runner"
    );
    assert!(
        linker_verification_step
            .lines()
            .map(str::trim)
            .any(|line| line == "mold --version"),
        "Act validation must invoke mold on the outer Linux runner"
    );
    assert!(
        test_step
            .lines()
            .map(str::trim)
            .any(|line| line == "run: make test WITH_ACT=1"),
        "Act validation must run Cargo's Act-enabled test path after linker verification"
    );
    assert!(
        linker_verification_step_offset < test_step_offset,
        "the outer runner must verify linkers before it runs the Act-enabled Cargo tests"
    );
    assert!(
        act_installation_step_offset < test_step_offset,
        "the outer runner must install act before it runs the Act-enabled Cargo tests"
    );
}

/// The Act-enabled test path executes the generated CI workflow's build-test job.
#[test]
fn act_enabled_tests_execute_the_ci_workflow() {
    let makefile = include_str!("../Makefile");
    let act_validation = make_conditional_body(makefile, "$(WITH_ACT),1");

    assert!(
        act_validation.lines().map(str::trim).any(|line| line
            == "act pull_request --workflows .github/workflows/ci.yml --job build-test \
                --platform ubuntu-latest=catthehacker/ubuntu:act-latest"),
        "WITH_ACT=1 must execute CI's build-test job through act without an interactive image \
         prompt"
    );
}
