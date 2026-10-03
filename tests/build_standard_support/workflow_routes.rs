//! Structural reader for Linux workflow routes that execute Rust test suites.
//!
//! The reader intentionally consumes the complete checked-in workflow corpus.
//! It recognizes the GitHub Actions forms used for runner selection and refuses
//! an ambiguous route instead of assuming that a new job is harmless.
use super::ci_steps::{Problems, SETUP_RUST_ACTION_PREFIX};
#[path = "workflow_routes/runner.rs"]
mod runner;
use runner::runs_on_linux;
/// One job extracted from a workflow's top-level `jobs` mapping.
pub(super) struct Job {
    pub(super) name: String,
    pub(super) body: String,
}
/// One workflow filename used in diagnostics.
#[derive(Clone, Copy)]
struct WorkflowName<'a>(&'a str);
/// One complete workflow source document.
#[derive(Clone, Copy)]
struct WorkflowText<'a>(&'a str);
/// One YAML source line.
#[derive(Clone, Copy)]
struct SourceLine<'a>(&'a str);
/// One parsed workflow step block.
#[derive(Clone, Copy)]
struct StepBlock<'a>(&'a str);
/// The checked-in workflow corpus available to recursive route traversal.
#[derive(Clone, Copy)]
struct WorkflowCorpus<'a>(&'a [(&'a str, &'a str)]);
/// A local reusable workflow path read from a caller job.
#[derive(Clone, Copy)]
struct WorkflowPath<'a>(&'a str);
/// Returns the leading-space indentation of a YAML line.
pub(super) fn indent(line: &str) -> usize { line.len() - line.trim_start().len() }
/// Extracts the top-level jobs from one GitHub Actions workflow.
fn jobs(name: WorkflowName<'_>, workflow: WorkflowText<'_>) -> Result<Vec<Job>, String> {
    let lines: Vec<&str> = workflow.0.lines().collect();
    let Some(jobs_at) = jobs_mapping_start(&lines) else {
        return Err(format!("{}: no top-level jobs mapping", name.0));
    };
    let mut result = Vec::new();
    let mut at = jobs_at + 1;
    while at < lines.len() {
        let line = job_source_line(name, &lines, at)?;
        if ends_jobs_mapping(line) {
            break;
        }
        if is_job_mapping_entry(line) {
            let (job, end) = read_job(name, &lines, at, line)?;
            result.push(job);
            at = end;
            continue;
        }
        at += 1;
    }
    if result.is_empty() {
        return Err(format!("{}: jobs mapping contains no jobs", name.0));
    }
    Ok(result)
}

/// Finds the source line that opens the top-level jobs mapping.
fn jobs_mapping_start(lines: &[&str]) -> Option<usize> {
    lines
        .iter()
        .position(|line| indent(line) == 0 && line.trim() == "jobs:")
}

/// Returns one source line or a stable diagnostic for an impossible offset.
fn job_source_line<'a>(
    name: WorkflowName<'_>,
    lines: &'a [&str],
    at: usize,
) -> Result<SourceLine<'a>, String> {
    lines
        .get(at)
        .copied()
        .map(SourceLine)
        .ok_or_else(|| format!("{}: jobs mapping contains an invalid line offset", name.0))
}

/// Returns whether a line closes the top-level jobs mapping.
fn ends_jobs_mapping(line: SourceLine<'_>) -> bool {
    !line.0.trim().is_empty() && indent(line.0) == 0
}

/// Returns whether a line opens one two-space-indented job mapping.
fn is_job_mapping_entry(line: SourceLine<'_>) -> bool {
    let trimmed = line.0.trim();
    indent(line.0) == 2 && trimmed.ends_with(':') && !trimmed.starts_with('-')
}

/// Reads one job and returns its exclusive source end.
fn read_job(
    name: WorkflowName<'_>,
    lines: &[&str],
    at: usize,
    line: SourceLine<'_>,
) -> Result<(Job, usize), String> {
    let job_name = line.0.trim().trim_end_matches(':').to_owned();
    let end = job_mapping_end(lines, at);
    let body_start = at.saturating_add(1);
    let Some(body_lines) = lines.get(body_start..end) else {
        return Err(format!(
            "{}: job `{job_name}` has an invalid source range",
            name.0
        ));
    };
    Ok((
        Job {
            name: job_name,
            body: body_lines.join("\n"),
        },
        end,
    ))
}

/// Finds the next job or the end of the source document.
fn job_mapping_end(lines: &[&str], at: usize) -> usize {
    lines
        .iter()
        .enumerate()
        .skip(at + 1)
        .find(|(_, next_line)| !next_line.trim().is_empty() && indent(next_line) <= 2)
        .map_or(lines.len(), |(index, _)| index)
}

/// Splits a job's `steps` sequence into source-order blocks.
fn step_blocks(job: &Job) -> Vec<(usize, String)> {
    let lines: Vec<&str> = job.body.lines().collect();
    let mut blocks = Vec::new();
    for (at, line) in lines.iter().enumerate() {
        if !line.trim_start().starts_with("- ") {
            continue;
        }
        let step_indent = indent(line);
        let end = (at + 1..lines.len())
            .find(|next| {
                lines.get(*next).is_some_and(|next_line| {
                    !next_line.trim().is_empty()
                        && indent(next_line) <= step_indent
                        && next_line.trim_start().starts_with("- ")
                })
            })
            .unwrap_or(lines.len());
        let block = lines
            .iter()
            .skip(at)
            .take(end.saturating_sub(at))
            .copied()
            .collect::<Vec<_>>()
            .join("\n");
        if block.contains("run:") || block.contains("uses:") {
            blocks.push((at, block));
        }
    }
    blocks
}

/// Returns whether one block starts a test suite or a coverage test suite.
fn is_suite_step(block: StepBlock<'_>) -> bool {
    block.0.contains("make test")
        || block.0.contains("cargo test")
        || block.0.contains("nextest")
        || block.0.contains("generate-coverage@")
        || block.0.contains("llvm-cov")
}

/// Returns whether a provision block is unconditional and has the shared
/// setup-rust action and its build-tool input.
fn has_build_tool_provision(block: StepBlock<'_>) -> bool {
    block.0.lines().any(|line| {
        let trimmed = line.trim();
        !trimmed.starts_with('#')
            && trimmed
                .strip_prefix("- ")
                .unwrap_or(trimmed)
                .strip_prefix("uses:")
                .map(str::trim)
                .and_then(|value| value.strip_prefix(SETUP_RUST_ACTION_PREFIX))
                .is_some_and(|revision| !revision.trim().is_empty())
    }) && block.0.contains("install-mold: 'true'")
        && !block.0.lines().any(|line| {
            let trimmed = line.trim();
            trimmed.starts_with("if:") || trimmed.starts_with("continue-on-error:")
        })
}

/// Finds a local reusable workflow path from a caller job.
fn local_reusable_path(job: &Job) -> Option<&str> {
    job.body.lines().find_map(|line| {
        let value = line.trim().strip_prefix("uses:")?.trim();
        value.strip_prefix("./.github/workflows/")
    })
}

/// Checks the route from one direct Linux job to its first suite step.
fn direct_route_problems(workflow: WorkflowName<'_>, job: &Job) -> Problems {
    let steps = step_blocks(job);
    let Some((suite_at, _)) = steps
        .iter()
        .find(|(_, block)| is_suite_step(StepBlock(block)))
    else {
        return Vec::new();
    };
    if steps
        .iter()
        .any(|(at, block)| at < suite_at && has_build_tool_provision(StepBlock(block)))
    {
        Vec::new()
    } else {
        vec![format!(
            "{}: job `{}` reaches a Linux test suite without an unconditional shared setup-rust \
             provision with install-mold beforehand",
            workflow.0, job.name
        )]
    }
}

/// Checks a reusable caller's Linux setup commands. The shared mutation runner
/// executes them before its suite, so an ignored or conditional installer is
/// insufficient.
fn reusable_route_problems(workflow: WorkflowName<'_>, job: &Job) -> Problems {
    let has_installer = job.body.contains("make install-build-tools");
    let hidden = job.body.lines().any(|line| {
        let trimmed = line.trim();
        trimmed.starts_with("if:") || trimmed.starts_with("continue-on-error:")
    });
    if has_installer && !hidden {
        Vec::new()
    } else {
        vec![format!(
            "{}: reusable job `{}` must unconditionally run make install-build-tools before its \
             Linux suite",
            workflow.0, job.name
        )]
    }
}

/// Checks every reachable Linux suite route in one workflow. `visiting` is a
/// reusable-workflow call stack, so a cycle is an explicit contract failure
/// rather than unbounded recursion in the reader.
fn workflow_suite_problems(
    workflows: WorkflowCorpus<'_>,
    workflow_name: WorkflowName<'_>,
    workflow: WorkflowText<'_>,
    visiting: &mut Vec<String>,
) -> Problems {
    let mut problems = Vec::new();
    let jobs = match jobs(workflow_name, workflow) {
        Ok(jobs) => jobs,
        Err(error) => return vec![error],
    };
    for job in jobs {
        if let Some(path) = local_reusable_path(&job) {
            problems.extend(reusable_workflow_problems(
                workflows,
                workflow_name,
                WorkflowPath(path),
                visiting,
            ));
            continue;
        }
        problems.extend(job_route_problems(workflow_name, &job));
    }
    problems
}

/// Follows one local reusable workflow or returns its fail-closed diagnostic.
fn reusable_workflow_problems(
    workflows: WorkflowCorpus<'_>,
    workflow_name: WorkflowName<'_>,
    path: WorkflowPath<'_>,
    visiting: &mut Vec<String>,
) -> Problems {
    let Some((child_name, child)) = workflows.0.iter().find(|(name, _)| *name == path.0) else {
        return vec![format!(
            "{}: local reusable workflow `{}` is absent from the corpus",
            workflow_name.0, path.0
        )];
    };
    if visiting.iter().any(|name| name.as_str() == *child_name) {
        return vec![format!(
            "{}: local reusable workflow `{}` forms a call cycle",
            workflow_name.0, path.0
        )];
    }
    visiting.push((*child_name).to_owned());
    let problems = workflow_suite_problems(
        workflows,
        WorkflowName(child_name),
        WorkflowText(child),
        visiting,
    );
    visiting.pop();
    problems
}

/// Checks one ordinary or external-reusable job that can reach a test suite.
fn job_route_problems(workflow_name: WorkflowName<'_>, job: &Job) -> Problems {
    let has_reusable_setup = job.body.contains("uses:") && job.body.contains("setup-commands:");
    let has_direct_suite = step_blocks(job)
        .iter()
        .any(|(_, block)| is_suite_step(StepBlock(block)));
    if !has_reusable_setup && !has_direct_suite {
        return Vec::new();
    }
    match runs_on_linux(job) {
        Ok(true) if has_reusable_setup => reusable_route_problems(workflow_name, job),
        Ok(true) => direct_route_problems(workflow_name, job),
        Ok(false) => Vec::new(),
        Err(error) => vec![format!(
            "{}: job `{}` has a test route with {error}",
            workflow_name.0, job.name
        )],
    }
}

/// Returns failures for every reachable Linux suite route in a workflow corpus.
/// Local reusable callers are followed by filename; an unresolved local target
/// or call cycle fails closed. External reusable callers that declare setup
/// commands are treated as Linux routes when those commands install Linux
/// prerequisites.
pub fn suite_provisioning_problems(workflows: &[(&str, &str)]) -> Problems {
    let mut problems = Vec::new();
    for (workflow_name, workflow) in workflows {
        let mut visiting = vec![(*workflow_name).to_owned()];
        problems.extend(workflow_suite_problems(
            WorkflowCorpus(workflows),
            WorkflowName(workflow_name),
            WorkflowText(workflow),
            &mut visiting,
        ));
    }
    problems.sort_unstable();
    problems.dedup();
    problems
}
