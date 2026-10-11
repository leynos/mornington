//! Hermetic subprocess contracts for build-tool provisioning and preflight.

#![cfg(unix)]

mod build_tools_support;

use build_tools_support::{ScriptFixture, ScriptName, ToolName};
use cap_std::fs_utf8::camino::Utf8Path;
use rstest::{fixture, rstest};

/// Allocates a capability-owned repository whose external commands are controlled.
#[fixture]
fn scripts() -> Result<ScriptFixture, String> {
    let fixture = ScriptFixture::new()?;
    Ok(fixture)
}

/// Both approved Linux architectures install the real fixture archive, after
/// requesting every component declared by the copied toolchain.
#[rstest]
#[case::x86("x86_64")]
#[case::arm("aarch64")]
fn installer_requests_components_and_installs_verified_artefact(
    scripts: Result<ScriptFixture, String>,
    #[case] architecture: &str,
) -> Result<(), String> {
    let fixture = scripts?;
    let output = fixture.run(ScriptName::Install, &[], &[("HOST_ARCH", architecture)])?;
    if !output.status.success() {
        return Err(format!(
            "installer must succeed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let calls = fixture.read(Utf8Path::new("commands.log"))?;
    if !calls.contains(concat!(
        "rustup toolchain install nightly-2026-08-27 --profile minimal ",
        "--component clippy --component llvm-tools-preview ",
        "--component rustc-codegen-cranelift-preview --component rustfmt"
    )) {
        return Err(format!(
            "installer must request the declared nightly and all components: {calls}"
        ));
    }
    if !calls.contains(&format!("mold-2.41.0-{architecture}-linux.tar.gz")) {
        return Err(format!(
            "installer must download the approved architecture archive: {calls}"
        ));
    }
    let artefact = fixture.read(Utf8Path::new("prefix/bin/mold"))?;
    if artefact != "verified fixture linker artefact\n" {
        return Err(format!(
            "verified archive must produce its installed artefact, found {artefact:?}"
        ));
    }
    Ok(())
}

/// A checksum mismatch must fail before extraction or installation.
#[rstest]
fn installer_rejects_corrupt_download(
    scripts: Result<ScriptFixture, String>,
) -> Result<(), String> {
    let fixture = scripts?;
    fixture.write(
        Utf8Path::new("tools/mold/SHA256SUMS"),
        &format!("{}  mold-2.41.0-x86_64-linux.tar.gz\n", "0".repeat(64)),
    )?;
    let output = fixture.run(ScriptName::Install, &[], &[])?;
    if output.status.success() {
        return Err(format!(
            "mismatched checksum must fail installation: {output:?}"
        ));
    }
    if !String::from_utf8_lossy(&output.stderr).contains("checksum mismatch") {
        return Err(format!(
            "installer must diagnose the checksum failure: {output:?}"
        ));
    }
    let calls = fixture.read(Utf8Path::new("commands.log"))?;
    if calls.lines().any(|line| line.starts_with("tar ")) {
        return Err(format!("checksum failure must prevent extraction: {calls}"));
    }
    if let Ok(artefact) = fixture.read(Utf8Path::new("prefix/bin/mold")) {
        return Err(format!(
            "failed verification must leave no installed artefact, found {artefact:?}"
        ));
    }
    Ok(())
}

/// One independently unusable prerequisite exercised by the actual checker.
#[derive(Clone, Copy)]
enum UnusablePrerequisite {
    /// Rustup cannot report a required preview component as installed.
    Component,
    /// The executable exists but reports a different linker release.
    LinkerVersion,
    /// Linux cannot locate the project-pinned linker executable.
    MissingPinnedLinker,
    /// Coverage lacks its additional LLVM linker.
    CoverageLinker,
}

/// The actual preflight refuses missing components, mismatched pinned linker, and missing lld.
#[rstest]
#[case::components(UnusablePrerequisite::Component)]
#[case::linker(UnusablePrerequisite::LinkerVersion)]
#[case::missing_linker(UnusablePrerequisite::MissingPinnedLinker)]
#[case::coverage(UnusablePrerequisite::CoverageLinker)]
fn checker_fails_unusable_prerequisites(
    scripts: Result<ScriptFixture, String>,
    #[case] prerequisite: UnusablePrerequisite,
) -> Result<(), String> {
    let fixture = scripts?;
    let (arguments, overrides, diagnostic): (&[&str], &[(&str, &str)], &str) = match prerequisite {
        UnusablePrerequisite::Component => (
            &[],
            &[("MISSING_COMPONENT", "llvm-tools")],
            "lacks llvm-tools-preview",
        ),
        UnusablePrerequisite::LinkerVersion => (
            &[],
            &[("LINKER_VERSION", "0.0.0")],
            "mold 2.41.0 is required",
        ),
        UnusablePrerequisite::MissingPinnedLinker => {
            fixture.remove_tool(ToolName::PinnedLinker)?;
            (&[], &[], "mold is required for the configured Linux linker")
        }
        UnusablePrerequisite::CoverageLinker => {
            fixture.remove_tool(ToolName::Lld)?;
            (&["--coverage"], &[], "ld.lld is required")
        }
    };
    let output = fixture.run(ScriptName::Check, arguments, overrides)?;
    if output.status.success() {
        return Err(format!("unusable prerequisites must fail: {output:?}"));
    }
    if !String::from_utf8_lossy(&output.stderr).contains(diagnostic) {
        return Err(format!(
            "preflight must diagnose {diagnostic:?}: {output:?}"
        ));
    }
    Ok(())
}

/// Complete prerequisites work for ordinary and coverage preflights; ordinary
/// builds do not acquire a dependency on the coverage-only linker.
#[rstest]
#[case::development(&[], false)]
#[case::coverage(&["--coverage"], false)]
#[case::development_without_lld(&[], true)]
fn checker_accepts_complete_prerequisites(
    scripts: Result<ScriptFixture, String>,
    #[case] arguments: &[&str],
    #[case] remove_lld: bool,
) -> Result<(), String> {
    let fixture = scripts?;
    if remove_lld {
        fixture.remove_tool(ToolName::Lld)?;
    }
    let output = fixture.run(ScriptName::Check, arguments, &[])?;
    if !output.status.success() {
        return Err(format!("complete prerequisites must pass: {output:?}"));
    }
    Ok(())
}

/// Non-Linux hosts still install and check Rust while skipping Linux linkers.
#[rstest]
#[case::install(ScriptName::Install, &[])]
#[case::check(ScriptName::Check, &[])]
#[case::coverage(ScriptName::Check, &["--coverage"])]
fn non_linux_routes_do_not_require_linux_linkers(
    scripts: Result<ScriptFixture, String>,
    #[case] script: ScriptName,
    #[case] arguments: &[&str],
) -> Result<(), String> {
    let fixture = scripts?;
    for tool in [
        ToolName::Clang,
        ToolName::PinnedLinker,
        ToolName::Lld,
        ToolName::Curl,
        ToolName::Tar,
    ] {
        fixture.remove_tool(tool)?;
    }
    let output = fixture.run(script, arguments, &[("HOST_OS", "Darwin")])?;
    if !output.status.success() {
        return Err(format!(
            "non-Linux route must skip Linux prerequisites: {output:?}"
        ));
    }
    let calls = fixture.read(Utf8Path::new("commands.log"))?;
    if !calls.contains("rustup ") {
        return Err(format!("non-Linux route must still exercise Rust: {calls}"));
    }
    if calls.contains("curl ") || calls.contains("tar ") {
        return Err(format!(
            "non-Linux route must not download or extract the pinned linker: {calls}"
        ));
    }
    Ok(())
}

/// An installation refusal at the architecture or download boundary.
#[derive(Clone, Copy)]
enum InstallerFailure {
    /// The controlled download command reports a failed transfer.
    Download,
    /// No approved archive exists for the supplied Linux architecture.
    Architecture,
}

/// Installer boundary crossed by one recorded external command.
#[derive(Debug, PartialEq, Eq)]
enum InstallerStage {
    /// The installer attempted the archive transfer.
    Download,
    /// The installer attempted archive extraction into the prefix.
    Extraction,
}

/// Observable result of a refused install, including its downstream side effects.
#[derive(Debug, PartialEq, Eq)]
struct RefusedInstallation {
    /// A refusal must not report successful completion.
    succeeded: bool,
    /// The expected failure reason must be present in stderr.
    diagnosed: bool,
    /// Ordered download and extraction commands reached before refusal.
    stages: Vec<InstallerStage>,
    /// Neither failure mode may create an installed linker artefact.
    installed: bool,
}

/// Installer failures cannot continue to extract or install an archive.
#[rstest]
#[case::download(InstallerFailure::Download)]
#[case::unsupported_architecture(InstallerFailure::Architecture)]
fn installer_failures_stop_before_extraction(
    scripts: Result<ScriptFixture, String>,
    #[case] failure: InstallerFailure,
) -> Result<(), String> {
    let fixture = scripts?;
    let (overrides, diagnostic, stages): (&[(&str, &str)], &str, Vec<InstallerStage>) =
        match failure {
            InstallerFailure::Download => (
                &[("FAIL_DOWNLOAD", "1")],
                "fixture download refused",
                vec![InstallerStage::Download],
            ),
            InstallerFailure::Architecture => (
                &[("HOST_ARCH", "riscv64")],
                "no approved archive for Linux architecture riscv64",
                Vec::new(),
            ),
        };
    let output = fixture.run(ScriptName::Install, &[], overrides)?;
    let calls = fixture.read(Utf8Path::new("commands.log"))?;
    let artefact = fixture.read(Utf8Path::new("prefix/bin/mold"));
    let observed = RefusedInstallation {
        succeeded: output.status.success(),
        diagnosed: String::from_utf8_lossy(&output.stderr).contains(diagnostic),
        stages: calls
            .lines()
            .filter_map(|line| {
                line.strip_prefix("curl ")
                    .map(|_| InstallerStage::Download)
                    .or_else(|| {
                        line.strip_prefix("tar ")
                            .map(|_| InstallerStage::Extraction)
                    })
            })
            .collect(),
        installed: artefact.is_ok(),
    };
    let expected = RefusedInstallation {
        succeeded: false,
        diagnosed: true,
        stages,
        installed: false,
    };
    if observed != expected {
        return Err(format!(
            concat!(
                "installer refusal must produce {:?}, found {:?}; required diagnostic {:?}; ",
                "output {:?}; calls {}; artefact {:?}"
            ),
            expected, observed, diagnostic, output, calls, artefact
        ));
    }
    Ok(())
}
