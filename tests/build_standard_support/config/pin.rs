//! Strict parsing for the selected Rust toolchain channel.

use std::{error::Error, fmt};

/// The channel the toolchain file pins.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Pin {
    Nightly,
    Stable,
}

/// Why a toolchain file cannot select a build-standard channel.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum PinParseError {
    /// No channel declaration was found.
    Missing,
    /// More than one valid channel declaration was found.
    Repeated {
        /// Number of channel declarations found in the file.
        count: usize,
    },
    /// A channel declaration could not be read as one quoted TOML string.
    Malformed {
        /// One-based line number of the malformed declaration.
        line: usize,
    },
    /// The declaration names a channel outside the build standard.
    Unknown(String),
}

impl fmt::Display for PinParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Missing => formatter.write_str("rust-toolchain has no channel"),
            Self::Repeated { count } => {
                write!(formatter, "rust-toolchain has {count} channel declarations")
            }
            Self::Malformed { line } => {
                write!(
                    formatter,
                    "rust-toolchain line {line} has a malformed channel"
                )
            }
            Self::Unknown(channel) => {
                write!(
                    formatter,
                    "rust-toolchain channel `{channel}` is unsupported"
                )
            }
        }
    }
}

impl Error for PinParseError {}

impl Pin {
    /// Reads the pin from a `rust-toolchain.toml`.
    ///
    /// The channel must be named exactly once and be one the standard knows: a
    /// `nightly` (dated or not), `stable`, `beta`, or a numbered release.
    /// Anything else, and a missing, malformed, or repeated `channel`, is an
    /// error rather than a guess that lets a malformed file pass as stable.
    ///
    /// ```text
    /// Pin::read("channel = \"nightly-2026-05-28\"") == Ok(Pin::Nightly)
    /// Pin::read("channel = \"1.94.0\"")             == Ok(Pin::Stable)
    /// Pin::read("[toolchain]")                       == Err(..)
    /// ```
    ///
    /// # Errors
    ///
    /// Returns the reason when the channel is missing, malformed, repeated, or
    /// unsupported.
    pub fn read(toolchain: &str) -> Result<Self, PinParseError> {
        let mut declarations: Vec<Result<&str, PinParseError>> = toolchain
            .lines()
            .enumerate()
            .filter_map(|(index, line)| channel_declaration(line, index + 1))
            .collect();
        if declarations.is_empty() {
            return Err(PinParseError::Missing);
        }
        if declarations.len() > 1 {
            if let Some(error) = declarations
                .iter()
                .find_map(|declaration| declaration.as_ref().err())
            {
                return Err(error.clone());
            }
            return Err(PinParseError::Repeated {
                count: declarations.len(),
            });
        }
        let Some(channel) = declarations.pop() else {
            return Err(PinParseError::Missing);
        };
        Self::classify(channel?)
    }

    /// Classifies one supported channel name.
    fn classify(channel: &str) -> Result<Self, PinParseError> {
        let is_nightly = channel == "nightly" || is_dated_nightly(channel);
        let is_release = channel.split('.').count() >= 2
            && channel
                .split('.')
                .all(|part| !part.is_empty() && part.chars().all(|c| c.is_ascii_digit()));
        if is_nightly {
            Ok(Self::Nightly)
        } else if is_release || matches!(channel, "stable" | "beta") {
            Ok(Self::Stable)
        } else {
            Err(PinParseError::Unknown(channel.to_owned()))
        }
    }

    /// Returns whether the pin takes `-Zthreads`, which is a nightly flag.
    pub const fn takes_threads(self) -> bool { matches!(self, Self::Nightly) }
}

/// Returns a channel declaration, preserving malformed declarations for the
/// caller to reject instead of silently dropping them.
fn channel_declaration(line: &str, line_number: usize) -> Option<Result<&str, PinParseError>> {
    let declaration = without_toml_comment(line).trim();
    let remainder = declaration.strip_prefix("channel")?;
    if remainder
        .chars()
        .next()
        .is_some_and(|character| character != '=' && !character.is_whitespace())
    {
        return None;
    }
    let value = remainder.trim_start().strip_prefix('=').map(str::trim);
    Some(
        value.map_or(Err(PinParseError::Malformed { line: line_number }), |raw| {
            parse_channel_value(raw, line_number)
        }),
    )
}

/// Reads a quoted channel value and rejects trailing tokens or invalid names.
fn parse_channel_value(value: &str, line_number: usize) -> Result<&str, PinParseError> {
    let malformed = || PinParseError::Malformed { line: line_number };
    let Some(quote) = value
        .chars()
        .next()
        .filter(|quote| matches!(quote, '\'' | '"'))
    else {
        return Err(malformed());
    };
    let Some(after_opening_quote) = value.strip_prefix(quote) else {
        return Err(malformed());
    };
    let Some((channel, trailing)) = after_opening_quote.split_once(quote) else {
        return Err(malformed());
    };
    if !channel_value_is_well_formed(channel, trailing) {
        return Err(malformed());
    }
    Ok(channel)
}

/// Returns whether a channel value has no trailing text and a valid name.
fn channel_value_is_well_formed(channel: &str, trailing: &str) -> bool {
    trailing.trim().is_empty() && is_valid_channel_name(channel)
}

/// Returns whether a channel name contains only ASCII letters, digits, dots, or hyphens.
fn is_valid_channel_name(channel: &str) -> bool {
    !channel.is_empty() && channel.chars().all(is_valid_channel_character)
}

/// Returns whether one character is permitted in a channel name.
const fn is_valid_channel_character(character: char) -> bool {
    character.is_ascii_alphanumeric() || matches!(character, '.' | '-')
}

/// Removes a TOML comment without treating a `#` inside a quoted value as one.
fn without_toml_comment(line: &str) -> &str {
    let mut quote = None;
    let mut escaped = false;
    for (index, character) in line.char_indices() {
        if escaped {
            escaped = false;
        } else if quote == Some('"') && character == '\\' {
            escaped = true;
        } else if let Some(opening_quote) = quote {
            if character == opening_quote {
                quote = None;
            }
        } else if matches!(character, '\'' | '"') {
            quote = Some(character);
        } else if character == '#' {
            return line.get(..index).unwrap_or(line);
        }
    }
    line
}

/// Returns whether `channel` is an ISO-shaped dated nightly channel.
fn is_dated_nightly(channel: &str) -> bool {
    let Some(date) = channel.strip_prefix("nightly-") else {
        return false;
    };
    let parts: Vec<_> = date.split('-').collect();
    let [year, month, day] = parts.as_slice() else {
        return false;
    };
    year.len() == 4
        && month.len() == 2
        && day.len() == 2
        && [*year, *month, *day]
            .into_iter()
            .all(|part| part.chars().all(|character| character.is_ascii_digit()))
}
