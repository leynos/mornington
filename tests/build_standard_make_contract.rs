//! Contracts for Make's evaluated test route and build-preflight boundary.

#[path = "build_standard_support/config.rs"]
mod config;
#[path = "build_standard_support/make.rs"]
mod make;
#[path = "build_standard_support/make/probe.rs"]
mod make_probe;

use config::{CONFIG, Flags, Pin, THREADS_FLAG, TOOLCHAIN, config_problems};
use make::{
    Assignment,
    Host,
    MakeTarget,
    assigned_rustflags,
    commands_from,
    coverage_backend_problems,
    development_problems,
    held_out_problems,
    held_out_target_count,
    make_commands,
    make_output,
};
use make_probe::{
    evaluated_test_commands,
    failing_preflight_stops_test_suite,
    test_commands_preserve_caller_rustflags,
};
use rstest::rstest;

/// Returns an error listing every discovered contract violation.
fn none_of(problems: &[String]) -> Result<(), String> {
    if problems.is_empty() {
        Ok(())
    } else {
        Err(format!("{problems:#?}"))
    }
}

/// Builds the assignment a fixture line is expected to read as.
fn flags(words: &[&str], inherits: bool) -> Assignment {
    Assignment::Flags(Flags::from_words(words.iter().copied()), inherits)
}

/// A temporary `probe-cargo` makes `make --dry-run test` evaluate nextest and
/// doctest commands, including the `WITH_ACT` branch, without compiling.
#[test]
fn evaluated_test_routes_retain_every_standard_flag() -> Result<(), String> {
    let pin = Pin::read(TOOLCHAIN).map_err(|error| error.to_string())?;
    none_of(&config_problems(CONFIG, pin)?)?;
    let commands = evaluated_test_commands()?;
    if commands.is_empty() {
        return Err("the evaluated test route produced no Cargo commands".to_owned());
    }
    for command in commands {
        let Assignment::Flags(flags, inherits) = command else {
            return Err("an evaluated test Cargo command does not assign RUSTFLAGS".to_owned());
        };
        if !inherits {
            return Err("an evaluated test Cargo command drops caller RUSTFLAGS".to_owned());
        }
        flags.meets(pin, true)?;
    }
    Ok(())
}

/// The build prerequisite is a real ordering boundary: when it fails, the
/// Cargo suite is not started.
#[test]
fn failing_build_preflight_stops_the_test_suite() -> Result<(), String> {
    failing_preflight_stops_test_suite()
}

/// The executed `make test` route passes caller and development flags to both
/// Cargo commands without invoking a real compiler.
#[test]
fn make_test_executes_both_commands_with_caller_and_development_flags() -> Result<(), String> {
    test_commands_preserve_caller_rustflags()
}

/// Scenario: `make -n` output lines in each spelling of an assignment.
///
/// Invariant: a quoted assignment is read, with the caller's inherited flags set
/// aside, and a line assigning none reads as unassigned.
#[rstest]
#[case::plain("RUSTFLAGS=\"-D warnings -Zthreads=8\" cargo test", flags(&["-D", "warnings", THREADS_FLAG], false))]
#[case::inherited_flags_glued_on(
    "RUSTFLAGS=\"${RUSTFLAGS:+$RUSTFLAGS }-Zthreads=8\" cargo check",
    flags(&[THREADS_FLAG], true)
)]
#[case::inherited_flags_only("RUSTFLAGS=\"${RUSTFLAGS-}\" cargo build --release", flags(&[], true))]
#[case::no_assignment("cargo clippy --all-targets", Assignment::Unassigned)]
fn the_command_reader_reads_each_assignment(
    #[case] line: &str,
    #[case] expected: Assignment,
) -> Result<(), String> {
    if assigned_rustflags(line)? == expected {
        Ok(())
    } else {
        Err(format!("`{line}` was read wrongly"))
    }
}

/// Scenario: `make -n` output lines whose assignment the reader cannot parse.
///
/// Invariant: each is refused rather than passed, because an assignment in a
/// form the reader does not understand still replaces the configuration.
#[rstest]
#[case::unquoted("RUSTFLAGS=-Zthreads=8 cargo test")]
#[case::unterminated("RUSTFLAGS=\"-Zthreads=8 cargo test")]
fn the_command_reader_refuses_what_it_cannot_parse(#[case] line: &str) -> Result<(), String> {
    match assigned_rustflags(line) {
        Ok(_) => Err(format!("`{line}` was read, not refused")),
        Err(_) => Ok(()),
    }
}

#[test]
fn command_reader_ignores_cargo_paths_on_non_cargo_commands() -> Result<(), String> {
    let commands = commands_from(concat!(
        "unset CARGO_ENCODED_RUSTFLAGS; PATH=\"/home/test/.cargo/bin\" RUSTFLAGS=\"\" ",
        "whitaker --all -- --all-targets --all-features",
    ))?;
    if commands.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "Whitaker was read as a Cargo command: {commands:?}"
        ))
    }
}

#[test]
fn command_reader_recognizes_probe_cargo() -> Result<(), String> {
    let commands = commands_from("RUSTFLAGS=\"-Zpolonius=next\" probe-cargo test")?;
    let expected = vec![Assignment::Flags(
        Flags::from_words(["-Zpolonius=next"]),
        false,
    )];
    if commands == expected {
        Ok(())
    } else {
        Err(format!(
            "probe-cargo was not read as one Cargo command: {commands:?}"
        ))
    }
}

/// Scenario: a recipe continued over lines, beside an `echo` and another command.
///
/// Invariant: the continued command is one command, and lines that are not a
/// Cargo command are ignored.
#[test]
fn a_continued_command_is_one_command() -> Result<(), String> {
    let joined = commands_from(concat!(
        "RUSTFLAGS=\"-A\" \\\n",
        "cargo test\necho cargo test\nmake other\n"
    ))?;
    if joined == vec![flags(&["-A"], false)] {
        Ok(())
    } else {
        Err(format!("read wrongly: {joined:?}"))
    }
}

#[test]
fn development_targets_restate_the_flags_on_linux() -> Result<(), String> {
    let pin = Pin::read(TOOLCHAIN).map_err(|error| error.to_string())?;
    let (problems, read) = development_problems(Host::Linux, pin)?;
    none_of(&problems)?;
    if read == 0 {
        return Err(
            "no development target assigns RUSTFLAGS, so the check proves nothing".to_owned(),
        );
    }
    Ok(())
}

#[test]
fn development_targets_keep_the_frontend_but_not_the_linker_elsewhere() -> Result<(), String> {
    let pin = Pin::read(TOOLCHAIN).map_err(|error| error.to_string())?;
    none_of(&development_problems(Host::Darwin, pin)?.0)
}

/// Coverage measures and release ships, so both stay on the default flags. A
/// repository that lists no such target has nothing local to hold out, and the
/// check then reads no commands; otherwise it must read at least one.
#[test]
fn coverage_and_release_take_neither_flag() -> Result<(), String> {
    let (problems, read) = held_out_problems()?;
    none_of(&problems)?;
    if held_out_target_count() > 0 && read == 0 {
        return Err(
            "the held-out targets run no cargo command, so the check proves nothing".to_owned(),
        );
    }
    Ok(())
}

/// Coverage uses LLVM code generation while the held-out check keeps its
/// independent linker and development-flag exclusions.
#[test]
fn coverage_selects_llvm_codegen_and_rejects_backend_mutations() -> Result<(), String> {
    let output = make_output(MakeTarget::Coverage, Host::Linux)?;
    none_of(&coverage_backend_problems(&output))?;

    for (name, mutation) in [
        (
            "backend assignment removed",
            output.replace("CARGO_PROFILE_DEV_CODEGEN_BACKEND=llvm", ""),
        ),
        (
            "backend changed to Cranelift",
            output.replace(
                "CARGO_PROFILE_DEV_CODEGEN_BACKEND=llvm",
                "CARGO_PROFILE_DEV_CODEGEN_BACKEND=cranelift",
            ),
        ),
    ] {
        if mutation == output {
            return Err(format!(
                "the {name} mutation did not change the coverage command"
            ));
        }
        let problems = coverage_backend_problems(&mutation);
        if problems.len() != 1
            || problems
                .first()
                .is_none_or(|problem| !problem.contains("CARGO_PROFILE_DEV_CODEGEN_BACKEND=llvm"))
        {
            return Err(format!(
                "coverage contract did not identify the {name} mutation: {problems:#?}"
            ));
        }
    }
    Ok(())
}

/// Clippy must keep the repository-wide all-target, all-feature scope.
#[test]
fn clippy_uses_workspace_all_targets_and_all_features() -> Result<(), String> {
    let output = make_output(MakeTarget::LintClippy, Host::Linux)?.replace("\\\n", " ");
    let expected = "clippy --workspace --all-targets --all-features -- -D warnings";
    if output.contains(expected) {
        Ok(())
    } else {
        Err(format!(
            "lint-clippy must include `{expected}`; got:\n{output}"
        ))
    }
}

/// A release build keeps the caller's flags, warning denial, and Polonius,
/// while excluding the development frontend and linker. Mutations of the
/// assignment must make each part of that contract fail.
#[test]
fn release_route_retains_base_flags_and_rejects_mutations() -> Result<(), String> {
    for (host, name) in [(Host::Linux, "Linux"), (Host::Darwin, "Darwin")] {
        release_host_route_is_valid(host, name)?;
    }
    release_mutations_are_rejected()
}

/// Returns whether an evaluated release assignment keeps only its base flags.
fn is_release_route(assignment: &Assignment) -> bool {
    match assignment {
        Assignment::Flags(flags, inherits) => {
            *inherits
                && flags.names("-Dwarnings")
                && flags.names("-Zpolonius=next")
                && !flags.names_threads()
                && !flags.names_linker()
        }
        Assignment::Unassigned => false,
    }
}

/// Checks one host's evaluated release route.
fn release_host_route_is_valid(host: Host, name: &str) -> Result<(), String> {
    let commands = make_commands(MakeTarget::Release, host)?;
    let [command] = commands.as_slice() else {
        return Err(format!(
            "make release on {name} must print one Cargo command, got {commands:?}"
        ));
    };
    if is_release_route(command) {
        Ok(())
    } else {
        Err(format!(
            "make release on {name} lost caller, warning, or Polonius flags, or acquired \
             development flags: {command:?}"
        ))
    }
}

/// Proves each flag or inheritance mutation invalidates the release route.
fn release_mutations_are_rejected() -> Result<(), String> {
    let approved =
        "RUSTFLAGS=\"${RUSTFLAGS:+$RUSTFLAGS }-D warnings -Zpolonius=next\" cargo build --release";
    if !is_release_route(&assigned_rustflags(approved)?) {
        return Err("the approved release assignment failed before testing mutations".to_owned());
    }
    for (name, mutation) in [
        (
            "caller flags removed",
            approved.replace("${RUSTFLAGS:+$RUSTFLAGS }", ""),
        ),
        (
            "warning denial removed",
            approved.replace("-D warnings ", ""),
        ),
        (
            "Polonius flag removed",
            approved.replace(" -Zpolonius=next", ""),
        ),
        (
            "development frontend added",
            approved.replace("-Zpolonius=next", "-Zpolonius=next -Zthreads=8"),
        ),
        (
            "development linker added",
            approved.replace(
                "-Zpolonius=next",
                "-Zpolonius=next -C link-arg=-fuse-ld=mold",
            ),
        ),
    ] {
        if is_release_route(&assigned_rustflags(&mutation)?) {
            return Err(format!("release contract accepted the {name} mutation"));
        }
    }
    Ok(())
}
