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
    /// Coverage lacks its additional LLVM linker.
    CoverageLinker,
}

/// The actual preflight refuses missing components, mismatched pinned linker, and missing lld.
#[rstest]
#[case::components(UnusablePrerequisite::Component)]
#[case::linker(UnusablePrerequisite::LinkerVersion)]
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
