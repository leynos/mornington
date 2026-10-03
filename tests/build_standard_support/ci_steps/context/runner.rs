//! Interprets runner declarations for the parent CI workflow context.

use super::{Job, Text, indent};

/// One runner label after YAML list splitting.
#[derive(Clone, Copy)]
struct Label<'a>(&'a str);

impl Label<'_> {
    /// Returns the unquoted label with ASCII case folded.
    fn normalized(self) -> String {
        self.0
            .trim()
            .trim_matches(|character| matches!(character, '\'' | '"'))
            .to_ascii_lowercase()
    }
}

/// Platform meaning carried by one supported runner label.
#[derive(Clone, Copy)]
enum Platform {
    Linux,
    Other,
}

/// Aggregated platform evidence from a runner label list.
#[derive(Default)]
struct PlatformSummary {
    has_linux: bool,
    has_platform: bool,
}

impl PlatformSummary {
    /// Incorporates one recognized platform label.
    const fn add(&mut self, platform: Platform) {
        self.has_platform = true;
        self.has_linux |= matches!(platform, Platform::Linux);
    }

    /// Returns whether the labels select Linux or lack any platform entirely.
    fn finish(self) -> Result<bool, String> {
        if self.has_platform {
            Ok(self.has_linux)
        } else {
            Err("self-hosted runner has no platform label".to_owned())
        }
    }
}

impl Job<'_> {
    /// Resolves the job's scalar, mapping, or matrix runner declaration.
    pub(super) fn runs_on_linux(&self) -> Result<bool, String> {
        let Some((at, runner)) = runs_on_field(&self.lines) else {
            return Err("direct development job has no supported runs-on form".to_owned());
        };
        if runner.contains("matrix.") {
            return self.matrix_os_is_linux();
        }
        if !runner.trim().is_empty() {
            return labels_are_linux(runner);
        }
        mapping_runner_is_linux(&self.lines, at)
    }

    /// Resolves the explicit operating-system values in a matrix runner.
    fn matrix_os_is_linux(&self) -> Result<bool, String> {
        let labels = self
            .lines
            .iter()
            .filter_map(|line| matrix_os_value(line))
            .flat_map(split_labels)
            .collect::<Vec<_>>();
        if labels.is_empty() {
            return Err("matrix runner has no explicit os values".to_owned());
        }
        classified_labels_are_linux(labels)
    }
}

/// Finds the first runner declaration and its source position.
fn runs_on_field<'a>(lines: &'a [&'a str]) -> Option<(usize, &'a str)> {
    lines.iter().enumerate().find_map(|(at, line)| {
        line.trim()
            .strip_prefix("runs-on:")
            .map(|value| (at, value))
    })
}

/// Resolves a mapping-form runner declaration beneath an empty `runs-on` key.
fn mapping_runner_is_linux(lines: &[&str], at: usize) -> Result<bool, String> {
    let runner_indent = indent(Text(lines.get(at).copied().unwrap_or_default()));
    let values = lines
        .get(at + 1..)
        .unwrap_or_default()
        .iter()
        .take_while(|line| line.trim().is_empty() || indent(Text(line)) > runner_indent)
        .filter_map(|line| mapping_label_value(line))
        .collect::<Vec<_>>();
    if values.is_empty() {
        return Err("runs-on mapping has no labels".to_owned());
    }
    labels_are_linux(&values.join(","))
}

/// Extracts a sequence or `labels:` value from one mapping line.
fn mapping_label_value(line: &str) -> Option<&str> {
    let trimmed = line.trim();
    trimmed
        .strip_prefix("- ")
        .or_else(|| trimmed.strip_prefix("labels:"))
}

/// Extracts one matrix operating-system value.
fn matrix_os_value(line: &str) -> Option<&str> {
    let trimmed = line.trim();
    trimmed
        .strip_prefix("os:")
        .or_else(|| trimmed.strip_prefix("- os:"))
}

/// Splits a scalar or inline list into normalized label wrappers.
fn split_labels(value: &str) -> impl Iterator<Item = Label<'_>> {
    value
        .trim()
        .trim_matches(|character| matches!(character, '[' | ']'))
        .split(',')
        .map(str::trim)
        .filter(|label| !label.is_empty())
        .map(Label)
}

/// Resolves a scalar or inline-list runner declaration.
fn labels_are_linux(value: &str) -> Result<bool, String> {
    let labels = split_labels(value).collect::<Vec<_>>();
    if labels.is_empty() {
        return Err("runner labels are empty".to_owned());
    }
    classified_labels_are_linux(labels)
}

/// Classifies every label and aggregates its platform meaning.
fn classified_labels_are_linux(labels: Vec<Label<'_>>) -> Result<bool, String> {
    let mut summary = PlatformSummary::default();
    for label in labels {
        if let Some(platform) = classify_label(label)? {
            summary.add(platform);
        }
    }
    summary.finish()
}

/// Classifies one supported platform or self-hosted label.
fn classify_label(label: Label<'_>) -> Result<Option<Platform>, String> {
    let normalized = label.normalized();
    let is_linux =
        normalized == "linux" || normalized.contains("ubuntu") || normalized.contains("-linux");
    if is_linux {
        return Ok(Some(Platform::Linux));
    }
    if ["macos", "windows", "freebsd"]
        .iter()
        .any(|platform| normalized.contains(platform))
    {
        return Ok(Some(Platform::Other));
    }
    if normalized == "self-hosted" {
        Ok(None)
    } else {
        Err(format!("unsupported runner label `{}`", label.0))
    }
}
