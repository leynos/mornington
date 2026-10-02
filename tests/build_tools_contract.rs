//! Static contracts for the pinned development-build prerequisite scripts.

/// The generated build target definitions under contract.
const MAKEFILE: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/Makefile"));
/// The provisioner that downloads and verifies the mold release archive.
const INSTALLER: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/scripts/install-build-tools.sh"
));
/// The probe that validates toolchain and linker availability.
const CHECKER: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/scripts/check-build-tools.sh"
));
/// The approved mold release version.
const LINKER_VERSION: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/tools/mold/VERSION"));
/// The approved mold release archive checksums.
const LINKER_HASHES: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tools/mold/SHA256SUMS"
));

/// The preflight must run before every development Cargo target can build.
#[test]
fn development_targets_require_the_build_preflight() {
    for target in ["build", "test", "lint", "typecheck"] {
        let declaration = format!("{target}: check-build-tools");
        assert!(
            MAKEFILE.contains(&declaration),
            "make {target} must require check-build-tools before Cargo commands"
        );
    }
    assert!(
        MAKEFILE.contains("coverage: check-coverage-tools"),
        "coverage must require its LLVM-aware prerequisite probe"
    );
    for target in ["fmt", "check-fmt"] {
        let declaration = format!("{target}: check-build-tools");
        assert!(
            !MAKEFILE.contains(&declaration),
            "make {target} must not require a compiler or linker preflight"
        );
    }
}

/// Composite and lint routes sequence cache-consuming gates under `make -j`.
#[test]
fn composite_and_lint_routes_recurse_in_order() {
    assert!(
        MAKEFILE.contains(".NOTPARALLEL: all"),
        "the composite route must declare its serial gate boundary"
    );
    let all = "all: ## Perform a comprehensive check of code\n\t+$(MAKE) check-fmt\n\t+$(MAKE) \
               lint\n\t+$(MAKE) test\n\t+$(MAKE) test-workflow-contracts\n\t+$(MAKE) spelling";
    assert!(
        MAKEFILE.contains(all),
        "make all must recurse through every gate in order rather than parallel prerequisites"
    );
    let lint = "lint: check-build-tools ## Run rustdoc, Clippy, and Whitaker \
                sequentially\n\t+$(MAKE) lint-clippy\n\t+$(MAKE) lint-whitaker";
    assert!(
        MAKEFILE.contains(lint),
        "make lint must run its two leaf targets in Clippy-then-Whitaker order"
    );
}

/// The installer follows provisioning with a probe, so success proves tools are usable.
#[test]
fn installer_runs_the_build_preflight_after_provisioning() {
    assert!(
        MAKEFILE.contains(
            "install-build-tools: ## Install the pinned toolchain and verified build tools"
        ),
        "the Makefile must expose the install-build-tools target"
    );
    assert!(
        MAKEFILE.contains("+$(MAKE) check-build-tools"),
        "install-build-tools must run check-build-tools after installation"
    );
}

/// The binary installer verifies an approved archive and never falls back to source.
#[test]
fn linker_installation_is_checksum_verified_and_binary_only() {
    assert_eq!(
        LINKER_VERSION.trim(),
        "2.41.0",
        "the recorded mold version must match the approved binary release"
    );
    for archive in [
        "mold-2.41.0-x86_64-linux.tar.gz",
        "mold-2.41.0-aarch64-linux.tar.gz",
    ] {
        assert!(
            LINKER_HASHES.lines().any(|line| line.ends_with(archive)),
            "the checksum manifest must pin {archive}"
        );
    }
    assert!(
        INSTALLER.contains("sha256sum") && INSTALLER.contains("checksum mismatch"),
        "the installer must reject an archive whose checksum differs from the manifest"
    );
    assert!(
        INSTALLER.contains("https://github.com/rui314/mold/releases/download"),
        "the installer must download only the approved mold release artefact"
    );
    assert!(
        !INSTALLER.contains("cargo install") && !INSTALLER.contains("cargo binstall"),
        "the mold installer must not introduce a source or package-manager fallback"
    );
}

/// The checker verifies the pinned nightly, Rust components, linkers, and lld for coverage.
#[test]
fn checker_covers_toolchain_and_linker_requirements() {
    for required_fragment in [
        "rustup run",
        "rustup component list",
        "command -v clang",
        "command -v mold",
        "command -v ld.lld",
        "--coverage",
    ] {
        assert!(
            CHECKER.contains(required_fragment),
            "check-build-tools must verify {required_fragment}"
        );
    }
}

/// Whitaker retains caller flags and adds the development flags to its Cargo work.
#[test]
fn whitaker_preserves_caller_and_development_rustflags() {
    let Some(invocation) = MAKEFILE
        .lines()
        .find(|line| line.contains("$(WHITAKER) --all -- $(CARGO_FLAGS)"))
    else {
        panic!("make lint must retain a Whitaker invocation");
    };
    assert!(
        invocation.contains("RUSTFLAGS=\"$${RUSTFLAGS:+$$RUSTFLAGS }$(DEV_RUST_FLAGS)\""),
        "Whitaker must receive the development Rust flags"
    );
    assert!(
        invocation.contains("$(CARGO_FLAGS)"),
        "Whitaker must check every target and feature"
    );
}

/// Mutation testing uses the same verified provisioner and never installs mold from apt.
#[test]
fn mutation_testing_uses_the_verified_linker_provisioner() {
    let workflow = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/.github/workflows/mutation-testing.yml"
    ));
    assert!(
        workflow.contains("make install-build-tools"),
        "mutation testing must provision the pinned toolchain and mold binary"
    );
    assert!(
        !workflow.contains("apt-get install --yes --no-install-recommends clang lld mold"),
        "mutation testing must not replace the pinned mold binary with an apt package"
    );
}

/// The checker maps requested preview aliases to Rustup's installed host names.
#[test]
fn component_aliases_require_the_exact_installed_host_name() {
    const CHECKER_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/scripts/check-build-tools.sh");
    const HOST: &str = "x86_64-unknown-linux-gnu";
    const INSTALLED: &str = concat!(
        "clippy-x86_64-unknown-linux-gnu\n",
        "llvm-tools-x86_64-unknown-linux-gnu\n",
        "rustc-codegen-cranelift-x86_64-unknown-linux-gnu\n",
        "rustfmt-x86_64-unknown-linux-gnu"
    );
    for (requested, expected) in [
        ("clippy", "clippy-x86_64-unknown-linux-gnu"),
        ("llvm-tools-preview", "llvm-tools-x86_64-unknown-linux-gnu"),
        (
            "rustc-codegen-cranelift-preview",
            "rustc-codegen-cranelift-x86_64-unknown-linux-gnu",
        ),
        ("rustfmt", "rustfmt-x86_64-unknown-linux-gnu"),
    ] {
        let status = std::process::Command::new("bash")
            .args([
                "-ceu",
                "source \"$1\"; expected=$(installed_component_name \"$2\" \"$3\"); test \
                 \"$expected\" = \"$4\"; component_is_installed \"$expected\" \"$5\"",
                "check-component-alias",
                CHECKER_PATH,
                requested,
                HOST,
                expected,
                INSTALLED,
            ])
            .status()
            .expect("the component alias probe must start Bash");
        assert!(
            status.success(),
            "the present {requested} alias must resolve to its installed host name"
        );
    }
    let absent = std::process::Command::new("bash")
        .args([
            "-ceu",
            "source \"$1\"; expected=$(installed_component_name \"$2\" \"$3\"); \
             component_is_installed \"$expected\" \"$4\"",
            "check-component-alias",
            CHECKER_PATH,
            "llvm-tools-preview",
            HOST,
            "clippy-x86_64-unknown-linux-gnu",
        ])
        .status()
        .expect("the absent component probe must start Bash");
    assert!(
        !absent.success(),
        "an absent component alias must fail the prerequisite probe"
    );
}
