//! Harmless Cargo probes for contracts that evaluate Make's test recipes.

use std::{
    ffi::OsString,
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

use cap_std::{
    ambient_authority,
    fs_utf8::{Dir, Permissions, PermissionsExt, camino::Utf8PathBuf},
};

use super::make::{Assignment, commands_from};

/// A temporary `probe-cargo` executable. It makes the Makefile select its
/// nextest branch while `--dry-run` keeps all actual Cargo invocations inert.
struct ProbeCargo {
    temporary_directory: Dir,
    directory_name: Utf8PathBuf,
    directory_path: Utf8PathBuf,
    record_path: Utf8PathBuf,
}

impl ProbeCargo {
    /// Creates the harmless executable used by evaluated-Make contracts.
    fn new() -> Result<Self, String> {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| format!("reading clock for probe-cargo: {error}"))?
            .as_nanos();
        let temporary_path = Utf8PathBuf::from_path_buf(std::env::temp_dir())
            .map_err(|path| format!("temporary directory {} is not UTF-8", path.display()))?;
        let temporary_directory = Dir::open_ambient_dir(&temporary_path, ambient_authority())
            .map_err(|error| format!("opening {temporary_path}: {error}"))?;
        let directory_name = Utf8PathBuf::from(format!("mornington-probe-cargo-{nonce}"));
        let directory_path = temporary_path.join(&directory_name);
        let record_path = directory_path.join("cargo-calls.log");
        temporary_directory
            .create_dir(&directory_name)
            .map_err(|error| format!("creating {directory_path}: {error}"))?;
        let executable = directory_name.join("probe-cargo");
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
            directory_name,
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
            .read_to_string(self.directory_name.join("cargo-calls.log"))
            .map_err(|error| format!("reading {}: {error}", self.record_path))
    }
}

impl Drop for ProbeCargo {
    fn drop(&mut self) {
        match self
            .temporary_directory
            .remove_dir_all(&self.directory_name)
        {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) if std::thread::panicking() => drop(error),
            Err(error) => panic!(
                "removing temporary probe directory {} failed: {error}",
                self.directory_path
            ),
        }
    }
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
    let output = Command::new("make")
        .args([
            "--always-make",
            "test",
            "CARGO=probe-cargo",
            "CHECK_BUILD_TOOLS=probe-cargo",
            "BUILD_HOST_OS=Linux",
        ])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .env("PATH", probe.path()?)
        .env("CARGO_RECORD", probe.record_path())
        .env("RUSTFLAGS", CALLER_FLAGS)
        .output()
        .map_err(|error| format!("running make test with recording Cargo: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "make test with recording Cargo failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let recorded_calls = probe.recorded_calls()?;
    let test_calls: Vec<_> = recorded_calls
        .lines()
        .filter_map(|call| call.split_once('\t'))
        .filter(|(_, arguments)| {
            arguments.starts_with("nextest run ") || arguments.starts_with("test --doc ")
        })
        .collect();
    let [suite, doctest] = test_calls.as_slice() else {
        return Err(format!(
            "make test must send suite and doctest commands to the stub, got {test_calls:?}"
        ));
    };
    if !suite.1.starts_with("nextest run ") || !doctest.1.starts_with("test --doc ") {
        return Err(format!("unexpected make test commands: {test_calls:?}"));
    }

    let required_flag_sequences: &[&[&str]] = &[
        &["-C", "target-cpu=native"],
        &["-D", "warnings"],
        &["-Zpolonius=next"],
        &["-Zthreads=8"],
        &["-C", "link-arg=-fuse-ld=mold"],
    ];
    for (flags, command) in test_calls {
        let actual: Vec<_> = flags.split_whitespace().collect();
        for required in required_flag_sequences {
            if !actual
                .windows(required.len())
                .any(|window| window == *required)
            {
                return Err(format!(
                    "make test command `{command}` did not receive `{required:?}` in RUSTFLAGS \
                     `{flags}`"
                ));
            }
        }
    }
    Ok(())
}
