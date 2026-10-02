//! Contract tests for Cargo's build flags and toolchain pin.
//!
//! Cargo applies one `rustflags` source rather than merging them, and an
//! assigned `RUSTFLAGS` replaces every source. The Linux table therefore needs
//! both the nightly frontend and the mold linker, while non-Linux builds need
//! the frontend alone. The reader is exercised against fixtures before the
//! repository configuration, so the contract proves it detects drift.

#[path = "build_standard_support/config.rs"]
mod config;
use config::{CONFIG, Pin, Problems, TOOLCHAIN, config_problems, pin::PinParseError};
use rstest::rstest;

/// Turns a list of complaints into a test result.
fn none_of(problems: &Problems) -> Result<(), String> {
    if problems.is_empty() {
        Ok(())
    } else {
        Err(format!("{problems:#?}"))
    }
}

/// A toolchain file pinning a nightly channel.
const NIGHTLY: &str = "[toolchain]\nchannel = \"nightly-2026-05-28\"\n";
/// A toolchain file pinning a stable channel.
const STABLE: &str = "[toolchain]\nchannel = \"1.94.0\"\n";
/// A toolchain file without a channel.
const MISSING_CHANNEL: &str = "[toolchain]\ncomponents = [\"clippy\"]\n";
/// A toolchain file with an unquoted channel value.
const MALFORMED_CHANNEL: &str = "[toolchain]\nchannel = nightly-2026-05-28\n";
/// A toolchain file with a channel declaration missing its assignment operator.
const MALFORMED_ASSIGNMENT: &str = "[toolchain]\nchannel \"nightly-2026-05-28\"\n";
/// A toolchain file with extra tokens after a quoted channel value.
const MALFORMED_TRAILING_VALUE: &str = "[toolchain]\nchannel = \"nightly\" extra\n";
/// A toolchain file whose duplicate channels make its mode ambiguous.
const AMBIGUOUS_CHANNEL: &str = concat!(
    "[toolchain]\nchannel = \"nightly-2026-05-28\"\n",
    "channel = \"1.94.0\"\n"
);
/// A valid declaration must not hide a second, malformed channel declaration.
const VALID_WITH_MALFORMED_CHANNEL: &str = concat!(
    "[toolchain]\nchannel = \"nightly-2026-05-28\"\n",
    "channel = stable\n"
);
/// A toolchain file naming a channel outside the supported build modes.
const UNKNOWN_CHANNEL: &str = "[toolchain]\nchannel = \"weekly\"\n";

/// A compliant nightly configuration: the frontend flag in every source and the
/// linker in the Linux-wide cfg table alone.
const NIGHTLY_OK: &str = concat!(
    "[build]\nrustflags = [\"-Zthreads=8\"]\n",
    "[target.x86_64-unknown-linux-gnu]\nlinker = \"clang\"\n",
    "[target.'cfg(target_os = \"linux\")']\n",
    "rustflags = [\"-Zthreads=8\", \"-Clink-arg=-fuse-ld=mold\"]\n"
);
/// The same, with the linker flag spelled as the `-C` pair Cargo also accepts.
const NIGHTLY_SPELLED_APART: &str = concat!(
    "[build]\nrustflags = [\"-Zthreads=8\"]\n",
    "[target.x86_64-unknown-linux-gnu]\nlinker = \"clang\"\n",
    "[target.'cfg(target_os = \"linux\")']\n",
    "rustflags = [\"-Zthreads=8\", \"-C\", \"link-arg=-fuse-ld=mold\"]\n"
);
/// A compliant stable configuration: mold alone, in the Linux-wide cfg table.
const STABLE_OK: &str = concat!(
    "[target.x86_64-unknown-linux-gnu]\nlinker = \"clang\"\n",
    "[target.'cfg(target_os = \"linux\")']\n",
    "rustflags = [\"-Clink-arg=-fuse-ld=mold\"]\n"
);
/// A nightly configuration that scopes mold to one Linux target triple only.
const TRIPLE_ONLY_LINUX: &str = concat!(
    "[build]\nrustflags = [\"-Zthreads=8\"]\n",
    "[target.x86_64-unknown-linux-gnu]\nlinker = \"clang\"\n",
    "rustflags = [\"-Zthreads=8\", \"-Clink-arg=-fuse-ld=mold\"]\n"
);

/// A nightly configuration whose `[build]` source lost the frontend flag, so it
/// is missing it and also differs from the Linux source.
const BUILD_LOSES_THREADS: &str = concat!(
    "[build]\nrustflags = [\"-Dwarnings\"]\n",
    "[target.x86_64-unknown-linux-gnu]\nlinker = \"clang\"\n",
    "[target.'cfg(target_os = \"linux\")']\n",
    "rustflags = [\"-Zthreads=8\", \"-Clink-arg=-fuse-ld=mold\"]\n"
);
/// A nightly configuration whose Linux table lost mold.
const LINUX_LOSES_LINKER: &str = concat!(
    "[build]\nrustflags = [\"-Zthreads=8\"]\n",
    "[target.x86_64-unknown-linux-gnu]\nlinker = \"clang\"\n",
    "[target.'cfg(target_os = \"linux\")']\n",
    "rustflags = [\"-Zthreads=8\"]\n"
);
/// A nightly configuration that names mold in `[build]`, beyond Linux.
const LINKER_IN_BUILD: &str = concat!(
    "[build]\nrustflags = [\"-Zthreads=8\", \"-Clink-arg=-fuse-ld=mold\"]\n",
    "[target.x86_64-unknown-linux-gnu]\nlinker = \"clang\"\n",
    "[target.'cfg(target_os = \"linux\")']\n",
    "rustflags = [\"-Zthreads=8\", \"-Clink-arg=-fuse-ld=mold\"]\n"
);
/// A nightly configuration with no `[build]` source for the other hosts.
const NO_BUILD_SOURCE: &str = concat!(
    "[target.x86_64-unknown-linux-gnu]\nlinker = \"clang\"\n",
    "[target.'cfg(target_os = \"linux\")']\n",
    "rustflags = [\"-Zthreads=8\", \"-Clink-arg=-fuse-ld=mold\"]\n"
);
/// A stable configuration that names the nightly-only frontend flag.
const STABLE_WITH_THREADS: &str = concat!(
    "[target.x86_64-unknown-linux-gnu]\nlinker = \"clang\"\n",
    "[target.'cfg(target_os = \"linux\")']\n",
    "rustflags = [\"-Zthreads=8\", \"-Clink-arg=-fuse-ld=mold\"]\n"
);
/// A `rustflags` array spread over several lines, which the reader refuses.
const SPREAD_ARRAY: &str = "[build]\nrustflags = [\n  \"-Zthreads=8\",\n]\n";

/// Checks that a fixture configuration draws the expected number of complaints.
fn draws(config: &str, pin: Pin, expected: usize) -> Result<(), String> {
    let found = config_problems(config, pin)?.len();
    if found == expected {
        Ok(())
    } else {
        Err(format!("{config:?}: {found} problems, not {expected}"))
    }
}

/// Scenario: configurations of each shape, read on a nightly and a stable pin.
///
/// Invariant: a compliant nightly file passes, and each way of losing the
/// frontend flag, removing its linker setting, adding one outside Linux, or
/// letting a source drift is reported; a stable pin refuses the frontend flag
/// it cannot take.
#[rstest]
#[case::compliant_nightly(NIGHTLY_OK, Pin::Nightly, 0)]
#[case::linker_spelled_as_a_pair(NIGHTLY_SPELLED_APART, Pin::Nightly, 0)]
#[case::compliant_stable(STABLE_OK, Pin::Stable, 0)]
#[case::triple_only_linux_scope(TRIPLE_ONLY_LINUX, Pin::Nightly, 2)]
#[case::build_loses_the_frontend(BUILD_LOSES_THREADS, Pin::Nightly, 2)]
#[case::linux_loses_the_linker(LINUX_LOSES_LINKER, Pin::Nightly, 1)]
#[case::linker_named_in_build(LINKER_IN_BUILD, Pin::Nightly, 1)]
#[case::no_build_source(NO_BUILD_SOURCE, Pin::Nightly, 1)]
#[case::stable_names_the_frontend(STABLE_WITH_THREADS, Pin::Stable, 1)]
#[case::empty_configuration("", Pin::Nightly, 3)]
fn the_configuration_reader_reports_each_defect(
    #[case] config: &str,
    #[case] pin: Pin,
    #[case] expected: usize,
) -> Result<(), String> {
    draws(config, pin, expected)
}

/// Scenario: a `rustflags` array spread over several lines.
///
/// Invariant: the reader refuses it, because reading half of an entry would let
/// a lost flag pass.
#[test]
fn a_rustflags_array_spread_over_lines_is_refused() -> Result<(), String> {
    match config_problems(SPREAD_ARRAY, Pin::Nightly) {
        Ok(_) => Err("a rustflags array spread over lines was read".to_owned()),
        Err(_) => Ok(()),
    }
}

/// Scenario: toolchain files pinning each kind of channel.
///
/// Invariant: only a complete, unique `nightly` channel reads as nightly, so
/// only it is asked to carry `-Zthreads`.
#[rstest]
#[case::nightly(NIGHTLY, Pin::Nightly)]
#[case::stable(STABLE, Pin::Stable)]
#[case::quoted_literal("[toolchain]\nchannel = 'beta' # a TOML literal string\n", Pin::Stable)]
fn the_pin_reader_tells_the_channels_apart(#[case] toolchain: &str, #[case] expected: Pin) {
    assert_eq!(
        Pin::read(toolchain),
        Ok(expected),
        "the complete channel should select the expected build mode"
    );
}

/// Scenario: malformed toolchain pins.
///
/// Invariant: missing, malformed, and ambiguous channel data is rejected
/// rather than silently weakening the build-standard checks to stable mode.
#[rstest]
#[case::missing(MISSING_CHANNEL, PinParseError::Missing)]
#[case::malformed(MALFORMED_CHANNEL, PinParseError::Malformed { line: 2 })]
#[case::malformed_assignment(MALFORMED_ASSIGNMENT, PinParseError::Malformed { line: 2 })]
#[case::malformed_trailing_value(MALFORMED_TRAILING_VALUE, PinParseError::Malformed { line: 2 })]
#[case::ambiguous(AMBIGUOUS_CHANNEL, PinParseError::Repeated { count: 2 })]
#[case::valid_then_malformed(
    VALID_WITH_MALFORMED_CHANNEL,
    PinParseError::Malformed { line: 3 }
)]
#[case::unknown(UNKNOWN_CHANNEL, PinParseError::Unknown("weekly".to_owned()))]
fn the_pin_reader_fails_closed_on_invalid_channel_data(
    #[case] toolchain: &str,
    #[case] expected: PinParseError,
) {
    assert!(
        Pin::read(toolchain) == Err(expected.clone()),
        "invalid toolchain channel data must return {expected:?}"
    );
}

#[test]
fn every_rustflags_source_is_consistent_with_the_pin() -> Result<(), String> {
    let pin = Pin::read(TOOLCHAIN).map_err(|error| error.to_string())?;
    none_of(&config_problems(CONFIG, pin)?)
}
