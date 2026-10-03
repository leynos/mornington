//! Interprets supported GitHub Actions Linux runner forms for route checks.

use super::{Job, indent};

/// One normalized GitHub Actions runner label.
#[derive(Clone, Copy)]
struct Label<'a>(&'a str);

impl Label<'_> {
    /// Returns the label without YAML quoting and with ASCII case folded.
    fn normalized(self) -> String {
        self.0
            .trim()
            .trim_matches(|character| matches!(character, '\'' | '\"'))
            .to_ascii_lowercase()
    }
}

/// A scalar or inline-list runner-label value.
#[derive(Clone, Copy)]
struct Labels<'a>(&'a str);

/// Normalizes one YAML runner label before comparing its meaning.
fn normalized_label(label: Label<'_>) -> String { label.normalized() }

/// Returns whether a label selects GitHub Actions self-hosted infrastructure.
fn is_self_hosted_label(label: Label<'_>) -> bool { normalized_label(label) == "self-hosted" }

/// Returns whether one label names Linux, a different supported OS, or neither.
fn is_linux_label(label: Label<'_>) -> Result<Option<bool>, String> {
    let normalized = normalized_label(label);
    let contains_any_platform = |platforms: &[&str]| {
        platforms
            .iter()
            .any(|platform| normalized.contains(*platform))
    };
    let has_linux = normalized == "linux" || contains_any_platform(&["ubuntu", "-linux"]);
    let has_other_os = contains_any_platform(&["macos", "windows", "freebsd"]);
    if has_linux && has_other_os {
        return Err(format!(
            "runner label \u{60}{}\u{60} names conflicting operating systems",
            label.0
        ));
    }
    Ok(if has_linux {
        Some(true)
    } else {
        has_other_os.then_some(false)
    })
}

/// Self-hosted metadata labels include architecture and organization labels.
/// They are accepted only beside an explicit label that establishes the OS.
fn is_self_hosted_metadata_label(label: Label<'_>) -> bool { !normalized_label(label).is_empty() }

/// Splits a scalar or inline YAML list into non-empty labels.
fn split_labels(value: Labels<'_>) -> impl Iterator<Item = Label<'_>> {
    value
        .0
        .trim()
        .trim_matches(|character| matches!(character, '[' | ']'))
        .split(',')
        .map(str::trim)
        .filter(|candidate| !candidate.is_empty())
        .map(Label)
}

/// Reads a scalar or inline YAML list of runner labels.
fn labels_are_linux(value: Labels<'_>) -> Result<bool, String> {
    let labels = split_labels(value).collect::<Vec<_>>();
    if labels.is_empty() {
        return Err("runner labels are empty".to_owned());
    }
    let has_self_hosted = labels.iter().copied().any(is_self_hosted_label);
    let operating_system = labels.into_iter().try_fold(None, |previous, label| {
        merge_runner_label(previous, label, has_self_hosted)
    })?;
    operating_system.ok_or_else(|| "self-hosted runner has no platform label".to_owned())
}

/// Incorporates one label into the selected operating system.
fn merge_runner_label(
    previous: Option<bool>,
    label: Label<'_>,
    has_self_hosted: bool,
) -> Result<Option<bool>, String> {
    if is_self_hosted_label(label) {
        return Ok(previous);
    }
    match is_linux_label(label)? {
        Some(next_os) => merge_operating_system(previous, next_os),
        None if has_self_hosted && is_self_hosted_metadata_label(label) => Ok(previous),
        None => Err(format!("unsupported runner label \u{60}{}\u{60}", label.0)),
    }
}

/// Merges one runner operating-system label, rejecting contradictory labels.
fn merge_operating_system(previous: Option<bool>, next: bool) -> Result<Option<bool>, String> {
    match previous {
        Some(previous_os) if previous_os != next => {
            Err("runner labels select conflicting operating systems".to_owned())
        }
        _ => Ok(Some(next)),
    }
}

/// Returns the matrix operating-system value declared by one YAML line.
fn matrix_os_value(line: &str) -> Option<&str> {
    let trimmed_line = line.trim();
    trimmed_line
        .strip_prefix("os:")
        .or_else(|| trimmed_line.strip_prefix("- os:"))
}

/// Collects every operating-system value declared by a matrix job.
fn matrix_os_values(job: &Job) -> Vec<Label<'_>> {
    job.body
        .lines()
        .filter_map(matrix_os_value)
        .flat_map(|value| split_labels(Labels(value)))
        .collect()
}

/// Checks matrix operating-system values without accepting self-hosted metadata.
fn matrix_values_are_linux(values: Vec<Label<'_>>) -> Result<bool, String> {
    let mut has_linux = false;
    for value in values {
        match is_linux_label(value)? {
            Some(is_linux) => has_linux |= is_linux,
            None => return Err(format!("unsupported matrix os \u{60}{}\u{60}", value.0)),
        }
    }
    Ok(has_linux)
}

/// Resolves matrix operating-system values from the job's matrix declaration.
fn matrix_os_is_linux(job: &Job) -> Result<bool, String> {
    let values = matrix_os_values(job);
    if values.is_empty() {
        Err("matrix runner has no explicit os values".to_owned())
    } else {
        matrix_values_are_linux(values)
    }
}

/// Reads runner labels from a mapping-form `runs-on` value.
fn mapping_labels_are_linux(lines: &[&str]) -> Result<bool, String> {
    let first_label = lines.iter().enumerate().find_map(|(at, line)| {
        let trimmed = line.trim();
        (trimmed.starts_with("labels:") || trimmed.starts_with("- ")).then_some((at, trimmed))
    });
    let Some((labels_at, labels_line)) = first_label else {
        return Err("runs-on mapping has no labels".to_owned());
    };
    if let Some(inline_labels) = labels_line.strip_prefix("labels:") {
        let trimmed_labels = inline_labels.trim();
        return if trimmed_labels.is_empty() {
            sequence_labels_are_linux(lines, labels_at + 1)
        } else {
            labels_are_linux(Labels(trimmed_labels))
        };
    }
    let values = lines
        .iter()
        .filter_map(|line| line.trim().strip_prefix("- "))
        .collect::<Vec<_>>();
    labels_are_linux(Labels(&values.join(",")))
}

/// Reads a nested sequence following an empty `labels:` field.
fn sequence_labels_are_linux(lines: &[&str], labels_at: usize) -> Result<bool, String> {
    let labels_indent = lines
        .get(labels_at.saturating_sub(1))
        .map_or(0, |line| indent(line));
    let values = lines
        .get(labels_at..)
        .unwrap_or_default()
        .iter()
        .take_while(|candidate| candidate.trim().is_empty() || indent(candidate) > labels_indent)
        .filter_map(|candidate| candidate.trim().strip_prefix("- "))
        .collect::<Vec<_>>();
    if values.is_empty() {
        return Err("runs-on labels list is empty".to_owned());
    }
    labels_are_linux(Labels(&values.join(",")))
}

/// Determines whether the job can run on Linux from scalar, list, mapping, or
/// matrix `runs-on` forms. A reusable caller with Linux setup commands is also
/// a Linux route, even though GitHub Actions does not allow `runs-on` beside
/// `uses` in that caller.
pub(super) fn runs_on_linux(job: &Job) -> Result<bool, String> {
    let lines: Vec<&str> = job.body.lines().collect();
    let Some((at, value)) = runs_on_field(&lines) else {
        return reusable_caller_is_linux(job);
    };
    explicit_runner_is_linux(job, &lines, at, value)
}

/// Finds the first `runs-on` field and its source position.
fn runs_on_field<'a>(lines: &'a [&'a str]) -> Option<(usize, &'a str)> {
    lines.iter().enumerate().find_map(|(at, line)| {
        line.trim()
            .strip_prefix("runs-on:")
            .map(|value| (at, value))
    })
}

/// Resolves one explicit scalar, matrix, or mapping runner declaration.
fn explicit_runner_is_linux(
    job: &Job,
    lines: &[&str],
    at: usize,
    value: &str,
) -> Result<bool, String> {
    if value.contains("matrix.") {
        return matrix_os_is_linux(job);
    }
    if !value.trim().is_empty() {
        return labels_are_linux(Labels(value));
    }
    let end = runner_mapping_end(lines, at);
    mapping_labels_are_linux(lines.get(at + 1..end).unwrap_or_default())
}

/// Returns the exclusive end of a mapping-form runner declaration.
fn runner_mapping_end(lines: &[&str], at: usize) -> usize {
    let parent_indent = lines.get(at).map_or(0, |line| indent(line));
    (at + 1..lines.len())
        .find(|next| {
            lines.get(*next).is_some_and(|next_line| {
                !next_line.trim().is_empty() && indent(next_line) <= parent_indent
            })
        })
        .unwrap_or(lines.len())
}

/// Resolves the implicit Linux route used by supported reusable callers.
fn reusable_caller_is_linux(job: &Job) -> Result<bool, String> {
    let has_setup_commands = job.body.contains("uses:") && job.body.contains("setup-commands:");
    if !has_setup_commands {
        return Err("job has no supported runs-on form".to_owned());
    }
    Ok(job.body.contains("apt-get") || job.body.contains("install-build-tools"))
}
