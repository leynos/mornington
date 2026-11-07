//! The build-tool scripts follow the channel that `rust-toolchain.toml` names.
//!
//! Dependabot moves the pin daily, so no test restates it: an unrelated nightly
//! written into the fixture's copied toolchain file must be the one the
//! installer installs and the one the checker runs.

#![cfg(unix)]

#[expect(
    dead_code,
    reason = "each test binary uses only a subset of the shared fixture"
)]
mod build_tools_support;

use build_tools_support::{ScriptFixture, ScriptName};
use cap_std::fs_utf8::camino::Utf8Path;

/// A channel no real pin can equal, so a leak of the original shows.
const CHANNEL: &str = "nightly-2030-01-02";

/// Turns a condition into a result that carries its explanation.
fn ensure(ok: bool, explain: impl FnOnce() -> String) -> Result<(), String> {
    ok.then_some(()).ok_or_else(explain)
}

/// Writes `CHANNEL` over the copied toolchain file and returns the old channel.
fn rewrite_channel(fixture: &ScriptFixture) -> Result<String, String> {
    let original = fixture.pinned_channel()?;
    let path = Utf8Path::new("rust-toolchain.toml");
    let toolchain = fixture.read(path)?;
    fixture.write(path, &toolchain.replace(&original, CHANNEL))?;
    Ok(original)
}

/// Runs one script and requires it to succeed.
fn run_ok(fixture: &ScriptFixture, script: ScriptName) -> Result<(), String> {
    let output = fixture.run(script, &[], &[("HOST_ARCH", "x86_64")])?;
    ensure(output.status.success(), || {
        format!("{script:?} must succeed: {output:?}")
    })
}

/// The recorded calls that are scoped to a toolchain.
fn scoped_calls(calls: &str) -> Vec<&str> {
    calls
        .lines()
        .filter(|line| {
            line.starts_with("rustup run ")
                || line.starts_with("rustup toolchain install ")
                || line.contains(" --toolchain ")
        })
        .collect()
}

#[test]
fn scripts_follow_the_channel_in_the_toolchain_file() -> Result<(), String> {
    let fixture = ScriptFixture::new()?;
    let original = rewrite_channel(&fixture)?;
    ensure(fixture.pinned_channel()? == CHANNEL, || {
        "pinned_channel must read the rewritten channel".to_owned()
    })?;
    run_ok(&fixture, ScriptName::Install)?;
    run_ok(&fixture, ScriptName::Check)?;
    let calls = fixture.read(Utf8Path::new("commands.log"))?;
    let scoped = scoped_calls(&calls);
    ensure(scoped.len() >= 3, || {
        format!("expected install and both probes, found {scoped:?}")
    })?;
    ensure(scoped.iter().all(|line| line.contains(CHANNEL)), || {
        format!("every scoped call must name {CHANNEL}: {scoped:?}")
    })?;
    // Each expected command, with the channel in its own argument position.
    for expected in [
        format!("rustup toolchain install {CHANNEL} "),
        format!("rustup run {CHANNEL} rustc --version"),
        format!("rustup run {CHANNEL} rustc -vV"),
        format!("rustup component list --toolchain {CHANNEL} "),
    ] {
        ensure(calls.contains(&expected), || {
            format!("missing {expected:?} in the recorded calls: {calls}")
        })?;
    }
    ensure(!calls.contains(&original), || {
        format!("the original channel {original} must not be used: {calls}")
    })
}
