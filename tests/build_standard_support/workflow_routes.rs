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
/// Returns the leading-space indentation of a YAML line.
pub(super) fn indent(line: &str) -> usize { line.len() - line.trim_start().len() }
/// Extracts the top-level jobs from one GitHub Actions workflow.
fn jobs(name: &str, workflow: &str) -> Result<Vec<Job>, String> {
    let lines: Vec<&str> = workflow.lines().collect();
    let Some(jobs_at) = lines
        .iter()
        .position(|line| indent(line) == 0 && line.trim() == "jobs:")
    else {
        return Err(format!("{name}: no top-level jobs mapping"));
    };
    let is_job_mapping_entry = |line: &str| {
        let trimmed = line.trim();
        indent(line) == 2 && trimmed.ends_with(':') && !trimmed.starts_with('-')
    };
    let mut result = Vec::new();
    let mut at = jobs_at + 1;
    while at < lines.len() {
        let Some(line) = lines.get(at).copied() else {
            return Err(format!(
                "{name}: jobs mapping contains an invalid line offset"
            ));
        };
        if !line.trim().is_empty() && indent(line) == 0 {
            break;
        }
        if is_job_mapping_entry(line) {
            let job_name = line.trim().trim_end_matches(':').to_owned();
            let end = lines
                .iter()
                .enumerate()
                .skip(at + 1)
                .find(|(_, next_line)| !next_line.trim().is_empty() && indent(next_line) <= 2)
                .map_or(lines.len(), |(index, _)| index);
            let body_start = at.saturating_add(1);
            let Some(body_lines) = lines.get(body_start..end) else {
                return Err(format!(
                    "{name}: job `{job_name}` has an invalid source range"
                ));
            };
            result.push(Job {
                name: job_name,
                body: body_lines.join("\n"),
            });
            at = end;
            continue;
        }
        at += 1;
    }
    if result.is_empty() {
        return Err(format!("{name}: jobs mapping contains no jobs"));
    }
    Ok(result)
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
fn is_suite_step(block: &str) -> bool {
    block.contains("make test")
        || block.contains("cargo test")
        || block.contains("nextest")
        || block.contains("generate-coverage@")
        || block.contains("llvm-cov")
}

/// Returns whether a provision block is unconditional and has the shared
/// setup-rust action and its build-tool input.
fn has_build_tool_provision(block: &str) -> bool {
    block.lines().any(|line| {
        let trimmed = line.trim();
        !trimmed.starts_with('#')
            && trimmed
                .strip_prefix("- ")
                .unwrap_or(trimmed)
                .strip_prefix("uses:")
                .map(str::trim)
                .and_then(|value| value.strip_prefix(SETUP_RUST_ACTION_PREFIX))
                .is_some_and(|revision| !revision.trim().is_empty())
    }) && block.contains("install-mold: 'true'")
        && !block.lines().any(|line| {
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
fn direct_route_problems(workflow: &str, job: &Job) -> Problems {
    let steps = step_blocks(job);
    let Some((suite_at, _)) = steps.iter().find(|(_, block)| is_suite_step(block)) else {
        return Vec::new();
    };
    if steps
        .iter()
        .any(|(at, block)| at < suite_at && has_build_tool_provision(block))
    {
        Vec::new()
    } else {
        vec![format!(
            "{workflow}: job `{}` reaches a Linux test suite without an unconditional shared \
             setup-rust provision with install-mold beforehand",
            job.name
        )]
    }
}

/// Checks a reusable caller's Linux setup commands. The shared mutation runner
/// executes them before its suite, so an ignored or conditional installer is
/// insufficient.
fn reusable_route_problems(workflow: &str, job: &Job) -> Problems {
    let has_installer = job.body.contains("make install-build-tools");
    let hidden = job.body.lines().any(|line| {
        let trimmed = line.trim();
        trimmed.starts_with("if:") || trimmed.starts_with("continue-on-error:")
    });
    if has_installer && !hidden {
        Vec::new()
    } else {
        vec![format!(
            "{workflow}: reusable job `{}` must unconditionally run make install-build-tools \
             before its Linux suite",
            job.name
        )]
    }
}

/// Checks every reachable Linux suite route in one workflow. `visiting` is a
/// reusable-workflow call stack, so a cycle is an explicit contract failure
/// rather than unbounded recursion in the reader.
fn workflow_suite_problems(
    workflows: &[(&str, &str)],
    workflow_name: &str,
    workflow: &str,
    visiting: &mut Vec<String>,
) -> Problems {
    let mut problems = Vec::new();
    let jobs = match jobs(workflow_name, workflow) {
        Ok(jobs) => jobs,
        Err(error) => return vec![error],
    };
    for job in jobs {
        if let Some(path) = local_reusable_path(&job) {
            let Some((child_name, child)) = workflows.iter().find(|(name, _)| *name == path) else {
                problems.push(format!(
                    "{workflow_name}: local reusable workflow `{path}` is absent from the corpus"
                ));
                continue;
            };
            if visiting.iter().any(|name| name.as_str() == *child_name) {
                problems.push(format!(
                    "{workflow_name}: local reusable workflow `{path}` forms a call cycle"
                ));
                continue;
            }
            visiting.push((*child_name).to_owned());
            problems.extend(workflow_suite_problems(
                workflows, child_name, child, visiting,
            ));
            visiting.pop();
            continue;
        }
        let has_reusable_setup = job.body.contains("uses:") && job.body.contains("setup-commands:");
        let has_direct_suite = step_blocks(&job)
            .iter()
            .any(|(_, block)| is_suite_step(block));
        if !has_reusable_setup && !has_direct_suite {
            continue;
        }
        match runs_on_linux(&job) {
            Ok(true) if has_reusable_setup => {
                problems.extend(reusable_route_problems(workflow_name, &job));
            }
            Ok(true) => problems.extend(direct_route_problems(workflow_name, &job)),
            Ok(false) => {}
            Err(error) => problems.push(format!(
                "{workflow_name}: job `{}` has a test route with {error}",
                job.name
            )),
        }
    }
    problems
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
            workflows,
            workflow_name,
            workflow,
            &mut visiting,
        ));
    }
    problems.sort_unstable();
    problems.dedup();
    problems
}
