//! Harmless Cargo probes for contracts that evaluate Make's test recipes.

use std::{ffi::OsString, process::Command};

use cap_std::{
    ambient_authority,
    fs_utf8::{Dir, Permissions, PermissionsExt, camino::Utf8PathBuf},
};

use super::make::{Assignment, commands_from};

/// A temporary `probe-cargo` executable. It makes the Makefile select its
/// nextest branch while `--dry-run` keeps all actual Cargo invocations inert.
struct ProbeCargo {
    temporary_directory: Dir,
    _temporary_directory_owner: tempfile::TempDir,
    directory_path: Utf8PathBuf,
    record_path: Utf8PathBuf,
}

impl ProbeCargo {
    /// Creates the harmless executable used by evaluated-Make contracts.
    fn new() -> Result<Self, String> {
        let owner = tempfile::Builder::new()
            .prefix("mornington-probe-cargo-")
            .tempdir()
            .map_err(|error| format!("allocating probe-cargo directory: {error}"))?;
        let directory_path = Utf8PathBuf::from_path_buf(owner.path().to_path_buf())
            .map_err(|path| format!("probe directory {} is not UTF-8", path.display()))?;
        let temporary_directory = Dir::open_ambient_dir(&directory_path, ambient_authority())
            .map_err(|error| format!("opening {directory_path}: {error}"))?;
        let record_path = directory_path.join("cargo-calls.log");
        let executable = Utf8PathBuf::from("probe-cargo");
        let executable_path = directory_path.join("probe-cargo");
        temporary_directory
            .write(
                &executable,
                concat!(
                    "#!/usr/bin/env sh\n",
                    "if [ \"$1\" = nextest ] && [ \"$2\" = --version ]; then echo cargo-nextest \
                     0.9.0; exit 0; fi\n",
                    "if [ -n \"${CARGO_RECORD:-}\" ]; then printf '%s\\t%s\\n' \"${RUSTFLAGS-}\" \
                     \"$*\" >> \"$CARGO_RECORD\"; fi\n",
                    "exit 0\n",
                ),
            )
            .map_err(|error| format!("writing {executable_path}: {error}"))?;
        #[cfg(unix)]
        {
            temporary_directory
                .set_permissions(&executable, Permissions::from_mode(0o755))
                .map_err(|error| format!("making {executable_path} executable: {error}"))?;
        }
        Ok(Self {
            temporary_directory,
            _temporary_directory_owner: owner,
            directory_path,
            record_path,
        })
    }

    /// Puts the probe first and keeps standard Unix tools available to Make.
    /// The dry-run needs its shell and `uname`, but no caller-specific tools.
    fn path(&self) -> Result<OsString, String> {
        let mut paths = vec![OsString::from(self.directory_path.as_str())];
        paths.extend(
            [
                "/usr/local/bin",
                "/usr/bin",
                "/bin",
                "/usr/local/sbin",
                "/usr/sbin",
                "/sbin",
                "/opt/homebrew/bin",
            ]
            .map(OsString::from),
        );
        std::env::join_paths(paths).map_err(|error| format!("constructing probe PATH: {error}"))
    }

    /// Returns the absolute path where the fake Cargo executable records calls.
    fn record_path(&self) -> &str { self.record_path.as_str() }

    /// Reads the calls recorded by the fake Cargo executable.
    fn recorded_calls(&self) -> Result<String, String> {
        self.temporary_directory
            .read_to_string("cargo-calls.log")
            .map_err(|error| format!("reading {}: {error}", self.record_path))
    }
}

/// One test command captured by the recording Cargo stub.
#[derive(Debug)]
struct RecordedCall<'a> {
    /// The exact flags the recipe supplied.
    rustflags: &'a str,
    /// The Cargo arguments the recipe supplied.
    arguments: &'a str,
}

/// Runs the evaluated test recipe with either arm of its `WITH_ACT` conditional.
fn evaluated_test_output(probe: &ProbeCargo, with_act: bool) -> Result<String, String> {
    let mut command = Command::new("make");
    command
        .args(["--dry-run", "test", "CARGO=probe-cargo"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .env("PATH", probe.path()?);
    if with_act {
        command.arg("WITH_ACT=1");
    }
    let output = command
        .output()
        .map_err(|error| format!("running evaluated make test: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "`make --dry-run test` with the probe-cargo executable failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Reads every Cargo invocation in both evaluated `test` conditional branches.
///
/// # Errors
///
/// Returns an error when the normal or Act branch is unreadable, a Cargo
/// invocation is missed, or either required test form disappears.
pub fn evaluated_test_commands() -> Result<Vec<Assignment>, String> {
    let probe = ProbeCargo::new()?;
    let normal = evaluated_test_output(&probe, false)?;
    let act = evaluated_test_output(&probe, true)?;
    if !act.contains("act pull_request") {
        return Err("WITH_ACT conditional was not evaluated by make --dry-run test".to_owned());
    }
    let mut commands = Vec::new();
    for output in [&normal, &act] {
        let expected = output
            .replace("\\\n", " ")
            .lines()
            .filter(|line| line.contains("probe-cargo"))
            .count();
        let parsed = commands_from(output)?;
        if parsed.len() != expected {
            return Err(format!(
                "parsed {} Cargo commands from {expected} evaluated probe-cargo invocations",
                parsed.len()
            ));
        }
        commands.extend(parsed);
    }
    if commands.len() < 4 {
        return Err(
            "both test conditional branches must retain nextest and doctest Cargo commands"
                .to_owned(),
        );
    }
    Ok(commands)
}

/// Proves the real prerequisite ordering with a harmless controlled failure.
///
/// # Errors
///
/// Returns an error when a failing build-tool probe allows any suite command
/// to be printed or the test target succeeds.
pub fn failing_preflight_stops_test_suite() -> Result<(), String> {
    let probe = ProbeCargo::new()?;
    let output = Command::new("make")
        .args([
            "--always-make",
            "test",
            "CARGO=probe-cargo",
            "CHECK_BUILD_TOOLS=false",
        ])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .env("PATH", probe.path()?)
        .output()
        .map_err(|error| format!("running controlled preflight failure: {error}"))?;
    if output.status.success() {
        return Err("a failing check-build-tools command let make test succeed".to_owned());
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    if stdout.contains("probe-cargo") {
        return Err("a failing check-build-tools command reached the Cargo test suite".to_owned());
    }
    Ok(())
}

/// Executes `make test` with a recording Cargo stub and checks both test calls.
///
/// The stub, rather than Cargo, receives the test and doctest commands so this
/// behavioural check validates the Make environment without compiling crates.
///
/// # Errors
/// Returns the reason when Make fails or either command loses caller or standard
/// development flags.
pub fn test_commands_preserve_caller_rustflags() -> Result<(), String> {
    const CALLER_FLAGS: &str = "-C target-cpu=native";
    let probe = ProbeCargo::new()?;
    run_recording_make_test(&probe, CALLER_FLAGS)?;
    validate_test_calls(&probe.recorded_calls()?)
}

/// Executes the test target with the recording Cargo stub.
fn run_recording_make_test(probe: &ProbeCargo, caller_flags: &str) -> Result<(), String> {
    // The outer suite runs Python contracts separately; this probe owns only
    // the two Cargo routes whose flags it records.
    let output = Command::new("make")
        .args([
            "--always-make",
            "test",
            "CARGO=probe-cargo",
            "CHECK_BUILD_TOOLS=probe-cargo",
            "PYTHON=true",
            "BUILD_HOST_OS=Linux",
        ])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .env("PATH", probe.path()?)
        .env("CARGO_RECORD", probe.record_path())
        .env("RUSTFLAGS", caller_flags)
        .output()
        .map_err(|error| format!("running make test with recording Cargo: {error}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "make test with recording Cargo failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}

/// Parses and validates the suite and doctest calls captured by the stub.
fn validate_test_calls(recorded_calls: &str) -> Result<(), String> {
    let test_calls: Vec<_> = recorded_calls
        .lines()
        .filter_map(|call| call.split_once('\t'))
        .filter(|(_, arguments)| {
            arguments.starts_with("nextest run ") || arguments.starts_with("test --doc ")
        })
        .map(|(rustflags, arguments)| RecordedCall {
            rustflags,
            arguments,
        })
        .collect();
    let [suite, doctest] = test_calls.as_slice() else {
        return Err(format!(
            "make test must send suite and doctest commands to the stub, got {test_calls:?}"
        ));
    };
    if !suite.arguments.starts_with("nextest run ") || !doctest.arguments.starts_with("test --doc ")
    {
        return Err(format!("unexpected make test commands: {test_calls:?}"));
    }

    validate_required_flags(&test_calls)
}

/// Checks every captured test command for the caller and development flags.
fn validate_required_flags(test_calls: &[RecordedCall<'_>]) -> Result<(), String> {
    let required_flag_sequences: &[&[&str]] = &[
        &["-C", "target-cpu=native"],
        &["-D", "warnings"],
        &["-Zpolonius=next"],
        &["-Zthreads=8"],
        &["-C", "link-arg=-fuse-ld=mold"],
    ];
    for call in test_calls {
        let actual: Vec<_> = call.rustflags.split_whitespace().collect();
        for required in required_flag_sequences {
            if !actual
                .windows(required.len())
                .any(|window| window == *required)
            {
                return Err(format!(
                    "make test command `{command}` did not receive `{required:?}` in RUSTFLAGS \
                     `{flags}`",
                    command = call.arguments,
                    flags = call.rustflags,
                ));
            }
        }
    }
    Ok(())
}
