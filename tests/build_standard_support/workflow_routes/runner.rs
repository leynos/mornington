//! Interprets supported GitHub Actions Linux runner forms for route checks.

use super::{Job, indent};

/// Normalizes one YAML runner label before comparing its meaning.
fn normalized_label(label: &str) -> String {
    label
        .trim()
        .trim_matches(|character| matches!(character, '\'' | '\"'))
        .to_ascii_lowercase()
}

/// Returns whether a label selects GitHub Actions self-hosted infrastructure.
fn is_self_hosted_label(label: &str) -> bool { normalized_label(label) == "self-hosted" }

/// Returns whether one label names Linux, a different supported OS, or neither.
fn is_linux_label(label: &str) -> Result<Option<bool>, String> {
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
            "runner label \u{60}{label}\u{60} names conflicting operating systems"
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
fn is_self_hosted_metadata_label(label: &str) -> bool { !normalized_label(label).is_empty() }

/// Splits a scalar or inline YAML list into non-empty labels.
fn split_labels(value: &str) -> impl Iterator<Item = &str> {
    value
        .trim()
        .trim_matches(|character| matches!(character, '[' | ']'))
        .split(',')
        .map(str::trim)
        .filter(|candidate| !candidate.is_empty())
}

/// Reads a scalar or inline YAML list of runner labels.
fn labels_are_linux(value: &str) -> Result<bool, String> {
    let labels = split_labels(value).collect::<Vec<_>>();
    if labels.is_empty() {
        return Err("runner labels are empty".to_owned());
    }
    let has_self_hosted = labels.iter().any(|label| is_self_hosted_label(label));
    let mut operating_system = None;
    for label in labels {
        if is_self_hosted_label(label) {
            continue;
        }
        match is_linux_label(label)? {
            Some(next_os) => operating_system = merge_operating_system(operating_system, next_os)?,
            None if has_self_hosted && is_self_hosted_metadata_label(label) => {}
            None => return Err(format!("unsupported runner label \u{60}{label}\u{60}")),
        }
    }
    operating_system.ok_or_else(|| "self-hosted runner has no platform label".to_owned())
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
fn matrix_os_values(job: &Job) -> Vec<&str> {
    job.body
        .lines()
        .filter_map(matrix_os_value)
        .flat_map(split_labels)
        .collect()
}

/// Checks matrix operating-system values without accepting self-hosted metadata.
fn matrix_values_are_linux(values: Vec<&str>) -> Result<bool, String> {
    let mut has_linux = false;
    for value in values {
        match is_linux_label(value)? {
            Some(is_linux) => has_linux |= is_linux,
            None => return Err(format!("unsupported matrix os \u{60}{value}\u{60}")),
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
            labels_are_linux(trimmed_labels)
        };
    }
    let values = lines
        .iter()
        .filter_map(|line| line.trim().strip_prefix("- "))
        .collect::<Vec<_>>();
    labels_are_linux(&values.join(","))
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
    labels_are_linux(&values.join(","))
}

/// Determines whether the job can run on Linux from scalar, list, mapping, or
/// matrix `runs-on` forms. A reusable caller with Linux setup commands is also
/// a Linux route, even though GitHub Actions does not allow `runs-on` beside
/// `uses` in that caller.
pub(super) fn runs_on_linux(job: &Job) -> Result<bool, String> {
    let lines: Vec<&str> = job.body.lines().collect();
    for (at, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        let Some(value) = trimmed.strip_prefix("runs-on:") else {
            continue;
        };
        if value.contains("matrix.") {
            return matrix_os_is_linux(job);
        }
        if !value.trim().is_empty() {
            return labels_are_linux(value);
        }
        let parent_indent = indent(line);
        let end = (at + 1..lines.len())
            .find(|next| {
                lines.get(*next).is_some_and(|next_line| {
                    !next_line.trim().is_empty() && indent(next_line) <= parent_indent
                })
            })
            .unwrap_or(lines.len());
        return mapping_labels_are_linux(lines.get(at + 1..end).unwrap_or_default());
    }
    if job.body.contains("uses:") && job.body.contains("setup-commands:") {
        return Ok(job.body.contains("apt-get") || job.body.contains("install-build-tools"));
    }
    Err("job has no supported runs-on form".to_owned())
}
