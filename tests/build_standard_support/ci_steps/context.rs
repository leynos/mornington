//! Typed, test-only line contexts used by the parent CI workflow reader.

#[path = "context/runner.rs"]
mod runner;

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
        match self.is_direct_linux_route() {
            Ok(false) => return Vec::new(),
            Err(error) => return vec![format!("{}: {error}", provision.workflow_name.0)],
            Ok(true) => {}
        }
        let first_route = self.first_direct_route();
        let setups = self.setup_steps();
        if setups.is_empty() {
            return vec![format!(
                "{}: the listed workflow has no setup-rust step, so the check proves nothing",
                provision.workflow_name.0
            )];
        }
        setups
            .into_iter()
            .flat_map(|(position, setup)| {
                setup.provision_problems(position, first_route, provision)
            })
            .collect()
    }

    /// Returns whether this job directly reaches development work on Linux.
    fn is_direct_linux_route(&self) -> Result<bool, String> {
        if self.is_direct_development_route() {
            self.runs_on_linux()
        } else {
            Ok(false)
        }
    }

    /// Returns the source-order position of the first direct development step.
    fn first_direct_route(&self) -> usize {
        self.steps
            .iter()
            .position(Step::direct_route)
            .unwrap_or_default()
    }

    /// Returns every setup-rust step with its source-order position.
    fn setup_steps(&self) -> Vec<(usize, &Step<'_>)> {
        self.steps
            .iter()
            .enumerate()
            .filter(|(_, step)| step.contains_action(Action::SetupRust))
            .collect()
    }
}

impl Step<'_> {
    /// Returns every provisioning failure contributed by this setup step.
    fn provision_problems(
        &self,
        position: usize,
        first_route: usize,
        provision: &Provision<'_>,
    ) -> Vec<String> {
        let mut problems = Vec::new();
        let workflow = provision.workflow_name.0;
        let line = self.index + 1;
        if position > first_route {
            problems.push(format!(
                "{workflow}:{line}: setup-rust must precede the direct development route"
            ));
        }
        if self.is_hidden() {
            problems.push(format!(
                "{workflow}:{line}: setup-rust must not be conditional or soft-failing"
            ));
        }
        if !self.installs_linker() {
            problems.push(format!(
                "{workflow}:{line}: a setup-rust step does not pass `install-mold: 'true'`"
            ));
        }
        if !self.uses_action_prefix(provision.setup_action_prefix) {
            problems.push(format!(
                "{workflow}:{line}: setup-rust does not use the shared action at {}",
                provision.setup_action_prefix.0
            ));
        }
        problems
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
