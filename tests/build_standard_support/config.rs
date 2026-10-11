//! Readers for the Cargo configuration half of the build standard: the
//! toolchain pin, the flag lists, and the `rustflags` sources in
//! `.cargo/config.toml`. Everything is read as text, so the contract needs no
//! parser dependency.

pub const CONFIG: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/.cargo/config.toml"));
pub const TOOLCHAIN: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/rust-toolchain.toml"));

/// The parallel-frontend flag every `rustflags` source carries on a nightly
/// pin.
pub const THREADS_FLAG: &str = "-Zthreads=8";
/// The linker flag the Linux source adds, normalized to one token.
pub const LINKER_FLAG: &str = "-Clink-arg=-fuse-ld=mold";

/// A list of complaints about the repository.
pub type Problems = Vec<String>;

#[path = "config/pin.rs"]
pub(crate) mod pin;
pub use pin::Pin;

/// A list of compiler flags, with `-C value` and `-D value` pairs joined so
/// both spellings compare equal.
#[derive(Debug, PartialEq, Eq)]
pub struct Flags(Vec<String>);

impl Flags {
    /// Reads a flag list from its words.
    ///
    /// ```text
    /// Flags::from_words(["-C", "link-arg=-fuse-ld=mold"]) == Flags::from_words(["-Clink-arg=-fuse-ld=mold"])
    /// Flags::from_words(["-D", "warnings"]) == Flags::from_words(["-Dwarnings"])
    /// ```
    pub fn from_words<'a>(words: impl IntoIterator<Item = &'a str>) -> Self {
        let mut joined: Vec<String> = Vec::new();
        for word in words {
            match joined.last_mut() {
                Some(last) if last == "-C" => *last = format!("-C{word}"),
                Some(last) if last == "-D" => *last = format!("-D{word}"),
                _ => joined.push(word.to_owned()),
            }
        }
        Self(joined)
    }

    /// Returns whether the list names one flag.
    pub fn names(&self, flag: &str) -> bool { self.0.iter().any(|candidate| candidate == flag) }

    /// Returns whether the list names the frontend flag.
    pub fn names_threads(&self) -> bool { self.names(THREADS_FLAG) }

    /// Returns whether the list names the linker flag.
    pub fn names_linker(&self) -> bool { self.names(LINKER_FLAG) }

    /// Returns the list without the linker flag, which is the one that may
    /// differ.
    fn without_linker_flag(&self) -> Vec<&String> {
        self.0.iter().filter(|flag| *flag != LINKER_FLAG).collect()
    }

    /// Checks the list against a pin and whether the linker flag is expected.
    ///
    /// ```text
    /// Flags::from_words(["-Zthreads=8"]).meets(Pin::Nightly, false) == Ok(())
    /// Flags::from_words([]).meets(Pin::Nightly, false).is_err()
    /// ```
    ///
    /// # Errors
    ///
    /// Returns the reason when the frontend or linker flag is wrong.
    pub fn meets(&self, pin: Pin, takes_linker_flag: bool) -> Result<(), String> {
        if self.names_threads() != pin.takes_threads() {
            return Err(format!("gets {THREADS_FLAG} wrong: {:?}", self.0));
        }
        if self.names_linker() != takes_linker_flag {
            return Err(format!("gets mold wrong: {:?}", self.0));
        }
        Ok(())
    }
}

/// Cargo table name for settings that apply to every Linux target.
const LINUX_CFG_TABLE: &str = "target.'cfg(target_os = \"linux\")'";

/// One `rustflags` source in a Cargo configuration.
struct Source {
    table: String,
    flags: Flags,
}

impl Source {
    /// Returns whether the table applies to every Linux target.
    fn is_linux(&self) -> bool { self.table == LINUX_CFG_TABLE }

    /// Returns what is wrong with the source's flags for a pin: the frontend
    /// flag on a nightly pin only, and mold in a Linux-wide cfg table only.
    fn problem(&self, pin: Pin) -> Option<String> {
        let reason = self.flags.meets(pin, self.is_linux()).err()?;
        Some(format!("[{}] {reason}", self.table))
    }
}

/// One line of a Cargo configuration, as far as the standard reads it.
enum Line {
    Table(String),
    Rustflags(Flags),
    Other,
}

/// Returns the quoted strings in one line, in order.
fn quoted(line: &str) -> Vec<&str> { line.split('"').skip(1).step_by(2).collect() }

/// Returns a line up to a `#` that starts a comment, ignoring a `#` inside a
/// quoted string, and without trailing space.
fn without_comment(line: &str) -> &str {
    let mut quote: Option<char> = None;
    for (index, c) in line.char_indices() {
        match (quote, c) {
            (None, '"' | '\'') => quote = Some(c),
            (Some(open), _) if c == open => quote = None,
            (None, '#') => return line.get(..index).unwrap_or(line).trim_end(),
            _ => {}
        }
    }
    line
}

/// Reads one configuration line. A `rustflags` entry is a one-line array of
/// strings, which is the shape the standard prescribes; an entry spread over
/// several lines is refused rather than half read.
///
/// ```text
/// read_line("[build]")                     -> Line::Table("build")
/// read_line("[build] # hosts")             -> Line::Table("build")
/// read_line("rustflags-extra = [\"x\"]")   -> Line::Other
/// read_line("rustflags = [\"-Zthreads=8\"]") -> Line::Rustflags(..)
/// ```
fn read_line(raw: &str) -> Result<Line, String> {
    let line = without_comment(raw);
    if line.starts_with('[') {
        return Ok(Line::Table(
            line.trim_matches(|c| c == '[' || c == ']')
                .trim()
                .to_owned(),
        ));
    }
    let Some(value) = line
        .strip_prefix("rustflags")
        .filter(|value| value.trim_start().starts_with('='))
    else {
        return Ok(Line::Other);
    };
    if !value.contains(']') {
        return Err("a `rustflags` array spans lines; keep it on one".to_owned());
    }
    Ok(Line::Rustflags(Flags::from_words(quoted(value))))
}

/// Returns every `rustflags` source in a Cargo configuration.
fn sources(config: &str) -> Result<Vec<Source>, String> {
    let mut table = String::new();
    let mut found = Vec::new();
    for line in config
        .lines()
        .map(str::trim)
        .filter(|line| !line.starts_with('#'))
    {
        match read_line(line)? {
            Line::Table(name) => table = name,
            Line::Rustflags(flags) => found.push(Source {
                table: table.clone(),
                flags,
            }),
            Line::Other => {}
        }
    }
    Ok(found)
}

/// Returns the complaints about which sources exist: there must be a Linux
/// table, and a nightly pin needs a `[build]` source for the other hosts.
fn shape_problems(found: &[Source], pin: Pin) -> Problems {
    let checks = [
        (found.is_empty(), "no rustflags source"),
        (
            !found.iter().any(Source::is_linux),
            "no Linux target table carries rustflags",
        ),
        (
            pin.takes_threads() && !found.iter().any(|source| source.table == "build"),
            "no [build] rustflags for non-Linux hosts",
        ),
    ];
    checks
        .into_iter()
        .filter(|(failed, _)| *failed)
        .map(|(_, text)| text.to_owned())
        .collect()
}

/// Returns a complaint when the sources differ in anything but the linker,
/// since Cargo applies one source rather than merging them.
fn drift_problem(found: &[Source]) -> Option<String> {
    let mut stripped: Vec<Vec<&String>> = found
        .iter()
        .map(|source| source.flags.without_linker_flag())
        .collect();
    stripped.dedup();
    (stripped.len() > 1).then(|| format!("sources differ beyond the linker: {stripped:?}"))
}

/// Returns every complaint about the configuration sources.
///
/// ```text
/// config_problems(CONFIG, Pin::Nightly) == Ok(vec![])   // a compliant repository
/// ```
///
/// # Errors
///
/// Returns the reason when the configuration cannot be read.
pub fn config_problems(config: &str, pin: Pin) -> Result<Problems, String> {
    let found = sources(config)?;
    let mut problems = shape_problems(&found, pin);
    problems.extend(found.iter().filter_map(|source| source.problem(pin)));
    problems.extend(drift_problem(&found));
    Ok(problems)
}
