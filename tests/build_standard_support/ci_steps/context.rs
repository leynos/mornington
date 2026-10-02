//! Typed, test-only line contexts used by the parent CI workflow reader.

/// Workflow actions inspected by the build-standard contract.
#[derive(Clone, Copy)]
pub(super) enum Action {
    SetupRust,
    Coverage,
    Whitaker,
}

/// The selected setup action and workflow name used in provision diagnostics.
pub(super) struct Provision<'a> {
    pub(super) setup_action_prefix: Text<'a>,
    pub(super) workflow_name: Text<'a>,
}

/// A YAML fragment passed through the private typed reader boundary.
#[derive(Clone, Copy)]
pub(super) struct Text<'a>(pub(super) &'a str);

/// A borrowed collection of YAML source lines.
#[derive(Clone, Copy)]
struct Lines<'list, 'source>(&'list [&'source str]);

impl Action {
    const fn marker(self) -> &'static str {
        match self {
            Self::SetupRust => "setup-rust@",
            Self::Coverage => "generate-coverage@",
            Self::Whitaker => "install-whitaker@",
        }
    }
}

/// One workflow step, retaining its source position for diagnostics.
pub(super) struct Step<'a> {
    pub(super) index: usize,
    lines: Vec<&'a str>,
}

impl Step<'_> {
    fn contains_action(&self, action: Action) -> bool {
        self.lines.iter().any(|line| {
            let trimmed = line.trim();
            !trimmed.starts_with('#')
                && trimmed
                    .strip_prefix("- ")
                    .unwrap_or(trimmed)
                    .strip_prefix("uses:")
                    .is_some_and(|value| value.contains(action.marker()))
        })
    }

    pub(super) fn has_line(&self, expected: Text<'_>) -> bool {
        self.lines.iter().any(|line| line.trim() == expected.0)
    }

    pub(super) fn uses_action_prefix(&self, expected: Text<'_>) -> bool {
        self.lines
            .iter()
            .filter_map(|line| {
                let trimmed = line.trim();
                trimmed
                    .strip_prefix("- ")
                    .unwrap_or(trimmed)
                    .strip_prefix("uses:")
            })
            .map(str::trim)
            .filter_map(|value| value.strip_prefix(expected.0))
            .any(|revision| !revision.trim().is_empty())
    }

    pub(super) fn has_compact(&self, expected: Text<'_>) -> bool {
        self.lines.iter().any(|line| {
            line.chars()
                .filter(|character| !matches!(character, ' ' | '\'' | '"'))
                .eq(expected.0.chars())
        })
    }

    pub(super) fn is_hidden(&self) -> bool {
        self.lines.iter().any(|line| {
            let trimmed = line.trim();
            trimmed.starts_with("if:") || trimmed.starts_with("continue-on-error:")
        })
    }

    pub(super) fn installs_linker(&self) -> bool { self.has_compact(Text("install-mold:true")) }

    pub(super) fn rustflags(&self) -> Option<&str> {
        self.lines
            .iter()
            .map(|line| line.trim())
            .find_map(|line| line.strip_prefix("RUSTFLAGS:"))
            .map(str::trim)
    }

    fn direct_route(&self) -> bool {
        self.contains_action(Action::Coverage)
            || self
                .lines
                .iter()
                .any(|line| is_development_command(Text(line.trim())))
    }
}

/// The lines belonging to one local workflow job.
struct Job<'a> {
    lines: Vec<&'a str>,
    steps: Vec<Step<'a>>,
}

impl Job<'_> {
    fn is_direct_development_route(&self) -> bool {
        !self.steps.is_empty() && self.steps.iter().any(Step::direct_route)
    }

    fn provision_problems(&self, provision: &Provision<'_>) -> Vec<String> {
        if !self.is_direct_development_route() {
            return Vec::new();
        }
        match self.runs_on_linux() {
            Ok(false) => return Vec::new(),
            Err(error) => return vec![format!("{}: {error}", provision.workflow_name.0)],
            Ok(true) => {}
        }
        let first_route = self
            .steps
            .iter()
            .position(Step::direct_route)
            .unwrap_or_default();
        let setups: Vec<_> = self
            .steps
            .iter()
            .enumerate()
            .filter(|(_, step)| step.contains_action(Action::SetupRust))
            .collect();
        if setups.is_empty() {
            return vec![format!(
                "{}: the listed workflow has no setup-rust step, so the check proves nothing",
                provision.workflow_name.0
            )];
        }
        let mut problems = Vec::new();
        for (position, setup) in setups {
            if position > first_route {
                problems.push(format!(
                    "{}:{}: setup-rust must precede the direct development route",
                    provision.workflow_name.0,
                    setup.index + 1
                ));
            }
            if setup.is_hidden() {
                problems.push(format!(
                    "{}:{}: setup-rust must not be conditional or soft-failing",
                    provision.workflow_name.0,
                    setup.index + 1
                ));
            }
            if !setup.installs_linker() {
                problems.push(format!(
                    "{}:{}: a setup-rust step does not pass `install-mold: 'true'`",
                    provision.workflow_name.0,
                    setup.index + 1
                ));
            }
            if !setup.uses_action_prefix(provision.setup_action_prefix) {
                problems.push(format!(
                    "{}:{}: setup-rust does not use the shared action at {}",
                    provision.workflow_name.0,
                    setup.index + 1,
                    provision.setup_action_prefix.0
                ));
            }
        }
        problems
    }

    fn runs_on_linux(&self) -> Result<bool, String> {
        let Some((at, runner)) = self.lines.iter().enumerate().find_map(|(at, line)| {
            line.trim()
                .strip_prefix("runs-on:")
                .map(|value| (at, value))
        }) else {
            return Err("direct development job has no supported runs-on form".to_owned());
        };
        if runner.contains("matrix.") {
            return self.matrix_os_is_linux();
        }
        if !runner.trim().is_empty() {
            return labels_are_linux(Text(runner));
        }
        let runner_indent = indent(Text(self.lines.get(at).copied().unwrap_or_default()));
        let values = self
            .lines
            .get(at + 1..)
            .unwrap_or_default()
            .iter()
            .take_while(|line| line.trim().is_empty() || indent(Text(line)) > runner_indent)
            .filter_map(|line| {
                let trimmed = line.trim();
                trimmed
                    .strip_prefix("- ")
                    .or_else(|| trimmed.strip_prefix("labels:"))
            })
            .collect::<Vec<_>>();
        if values.is_empty() {
            return Err("runs-on mapping has no labels".to_owned());
        }
        labels_are_linux(Text(&values.join(",")))
    }

    fn matrix_os_is_linux(&self) -> Result<bool, String> {
        let values = self.lines.iter().filter_map(|line| {
            let trimmed = line.trim();
            trimmed
                .strip_prefix("os:")
                .or_else(|| trimmed.strip_prefix("- os:"))
        });
        let labels = values
            .flat_map(|value| {
                value
                    .trim()
                    .trim_matches(|character| matches!(character, '[' | ']'))
                    .split(',')
                    .map(str::trim)
                    .filter(|label| !label.is_empty())
                    .map(str::to_owned)
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        if labels.is_empty() {
            return Err("matrix runner has no explicit os values".to_owned());
        }
        labels_are_linux(Text(&labels.join(",")))
    }
}

/// A typed view over one workflow document; it is private to the parent reader.
pub(super) struct WorkflowLines<'a> {
    lines: Vec<&'a str>,
}

impl<'a> WorkflowLines<'a> {
    pub(super) fn new(content: Text<'a>) -> Self {
        Self {
            lines: content.0.lines().collect(),
        }
    }

    pub(super) fn steps(&self, action: Action) -> Vec<Step<'a>> {
        self.all_steps()
            .into_iter()
            .filter(|step| step.contains_action(action))
            .collect()
    }

    pub(super) fn steps_with_text(&self, text: Text<'_>) -> Vec<Step<'a>> {
        self.all_steps()
            .into_iter()
            .filter(|step| step.has_line(text))
            .collect()
    }

    pub(super) fn development_problems(&self, provision: &Provision<'_>) -> Vec<String> {
        self.jobs()
            .into_iter()
            .flat_map(|job| job.provision_problems(provision))
            .collect()
    }

    fn jobs(&self) -> Vec<Job<'a>> {
        let Some(jobs_at) = self.lines.iter().position(|line| line.trim() == "jobs:") else {
            return Vec::new();
        };
        let mut jobs = Vec::new();
        let job_starts: Vec<_> = self
            .lines
            .get(jobs_at + 1..)
            .unwrap_or_default()
            .iter()
            .enumerate()
            .filter_map(|(offset, line)| is_job_header(Text(line)).then_some(jobs_at + offset + 1))
            .collect();
        for (position, start) in job_starts.iter().copied().enumerate() {
            let end = job_starts
                .get(position + 1)
                .copied()
                .unwrap_or(self.lines.len());
            let lines = self.lines.get(start..end).unwrap_or_default().to_vec();
            jobs.push(Job {
                steps: collect_steps(Lines(&lines), start),
                lines,
            });
        }
        jobs
    }

    fn all_steps(&self) -> Vec<Step<'a>> { collect_steps(Lines(&self.lines), 0) }
}

fn is_job_header(line: Text<'_>) -> bool {
    line.0.starts_with("  ")
        && !line.0.starts_with("    ")
        && line.0.trim_end().ends_with(':')
        && !line.0.trim_start().starts_with('#')
}

fn indent(line: Text<'_>) -> usize { line.0.len() - line.0.trim_start().len() }

fn labels_are_linux(value: Text<'_>) -> Result<bool, String> {
    let labels = value
        .0
        .trim()
        .trim_matches(|character| matches!(character, '[' | ']'))
        .split(',')
        .map(|label| {
            label
                .trim()
                .trim_matches(|character| matches!(character, '\'' | '"'))
        })
        .filter(|label| !label.is_empty())
        .collect::<Vec<_>>();
    if labels.is_empty() {
        return Err("runner labels are empty".to_owned());
    }
    let mut has_linux = false;
    let mut has_platform = false;
    for label in labels {
        let lower = label.to_ascii_lowercase();
        let is_linux_or_ubuntu = lower == "linux" || lower.contains("ubuntu");
        if is_linux_or_ubuntu || lower.contains("-linux") {
            has_linux = true;
            has_platform = true;
        } else if !["macos", "windows", "freebsd"]
            .iter()
            .any(|platform| lower.contains(platform))
        {
            if lower != "self-hosted" {
                return Err(format!("unsupported runner label `{label}`"));
            }
        } else {
            has_platform = true;
        }
    }
    if !has_platform {
        return Err("self-hosted runner has no platform label".to_owned());
    }
    Ok(has_linux)
}

fn collect_steps<'source>(lines: Lines<'_, 'source>, offset: usize) -> Vec<Step<'source>> {
    let starts: Vec<_> = lines
        .0
        .iter()
        .enumerate()
        .filter_map(|(index, line)| line.trim_start().starts_with("- ").then_some(index))
        .collect();
    starts
        .iter()
        .copied()
        .enumerate()
        .map(|(position, start)| {
            let end = starts.get(position + 1).copied().unwrap_or(lines.0.len());
            Step {
                index: offset + start,
                lines: lines.0.get(start..end).unwrap_or_default().to_vec(),
            }
        })
        .collect()
}

fn is_development_command(line: Text<'_>) -> bool {
    let content = line
        .0
        .trim_start()
        .strip_prefix("- ")
        .unwrap_or_else(|| line.0.trim_start());
    let command = content.strip_prefix("run: ").unwrap_or(content);
    let mut words = command.split_whitespace();
    match words.next() {
        Some("make") => matches!(
            words.next(),
            Some("build" | "test" | "lint" | "typecheck" | "audit")
        ),
        Some("cargo") => match words.next() {
            Some(toolchain) if toolchain.starts_with('+') => matches!(
                words.next(),
                Some("build" | "check" | "test" | "clippy" | "doc")
            ),
            Some("build" | "check" | "test" | "clippy" | "doc") => true,
            _ => false,
        },
        _ => false,
    }
}
