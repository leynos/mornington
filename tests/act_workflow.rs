//! Regression contract for the generated Act workflow's Linux linker setup.

/// The outer Cargo test process links binaries before any nested Act execution.
#[test]
fn act_validation_installs_the_configured_linker_before_tests() {
    let workflow = include_str!("../.github/workflows/act-validation.yml");
    let (setup, _) = workflow
        .split_once("run: make test WITH_ACT=1")
        .expect("Act validation must retain its Cargo test entrypoint");
    let installs_linker = setup.lines().any(|line| {
        line.contains("apt-get install")
            && line.split_whitespace().any(|word| word == "clang")
            && line.split_whitespace().any(|word| word == "mold")
    });
    assert!(
        installs_linker,
        "Act validation must install clang and mold before Cargo links test binaries"
    );
}
