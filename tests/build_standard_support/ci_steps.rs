//! Readers for CI's build-standard routes, including their Rust setup, coverage,
//! and Whitaker provisioners.

#[path = "ci_steps/context.rs"]
mod context;

use cap_std::{ambient_authority, fs_utf8::Dir};
use context::{Action, Provision, Step, Text, WorkflowLines};

/// The reader accumulates every violation so one test run can show the full
/// workflow drift rather than failing at the first line.
pub type Problems = Vec<String>;

/// Shared setup-rust action path; Dependabot owns the revision after `@`.
pub const SETUP_RUST_ACTION_PREFIX: &str = "leynos/shared-actions/.github/actions/setup-rust@";
/// The audited strict installer that rejects rolling suite pins and source
/// fallback.
pub const INSTALL_WHITAKER_ACTION: &str = "leynos/shared-actions/.github/actions/install-whitaker@\
                                           6dea5677a84fec60ca51b07202570e3af12ffdb4";
/// The selected coverage action revision shared by the pull-request and main lanes.
pub const COVERAGE_ACTION: &str = "leynos/shared-actions/.github/actions/generate-coverage@\
                                   d4d248bbbecdcf7b4f5bc79ffd4d6caee370bd79";

/// The coverage environment fields that keep instrumentation on LLVM and lld.
const COVERAGE_ENVIRONMENT: &[&str] = &[
    "CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER: clang",
    "CARGO_PROFILE_DEV_CODEGEN_BACKEND: llvm",
    "RUSTFLAGS: -Zpolonius=next -C link-arg=-fuse-ld=lld",
    "CFLAGS: -fuse-ld=lld",
    "LDFLAGS: -fuse-ld=lld",
];
/// The coverage action inputs common to the pull-request and main lanes.
const COVERAGE_INPUTS: &[&str] = &[
    "language: rust",
    "cargo-manifest: Cargo.toml",
    "all-targets: 'true'",
    "all-features: 'true'",
    "doctests: 'true'",
    "output-path: lcov.info",
    "format: lcov",
    "with-ratchet: 'true'",
];

/// One named workflow under the build-standard contract.
///
/// Reader methods borrow this context instead of passing a name and workflow
/// text separately, keeping each diagnostic tied to the content it inspects.
#[derive(Clone, Copy)]
pub struct Workflow<'a> {
    name: &'a str,
    content: &'a str,
}

impl<'a> Workflow<'a> {
    /// Creates a workflow reader for one named YAML document.
    pub const fn new(name: &'a str, content: &'a str) -> Self { Self { name, content } }

    /// Returns typed action steps from this workflow document.
    fn steps(&self, action: Action) -> Vec<Step<'a>> {
        WorkflowLines::new(Text(self.content)).steps(action)
    }

    /// Returns a complaint for every setup-rust step without `install-mold`.
    pub fn linker_install_problems(&self) -> Problems {
        self.steps(Action::SetupRust)
            .into_iter()
            .filter(|step| !step.installs_linker())
            .map(|step| {
                format!(
                    "{}:{}: a setup-rust step does not pass `install-mold: 'true'`",
                    self.name,
                    step.index + 1
                )
            })
            .collect()
    }

    /// Returns a complaint when a setup-rust step is outside the shared action.
    pub fn setup_rust_action_problems(&self) -> Problems {
        self.steps(Action::SetupRust)
            .into_iter()
            .filter(|step| !step.uses_action_prefix(Text(SETUP_RUST_ACTION_PREFIX)))
            .map(|step| {
                format!(
                    "{}:{}: setup-rust does not use the shared action at \
                     {SETUP_RUST_ACTION_PREFIX}",
                    self.name,
                    step.index + 1
                )
            })
            .collect()
    }

    /// Returns the complaint when any coverage step does not select LLVM explicitly.
    pub fn coverage_backend_problems(&self) -> Problems {
        let steps = self.steps(Action::Coverage);
        if steps.is_empty() {
            return vec![format!(
                "{}: no generate-coverage step, so the check proves nothing",
                self.name
            )];
        }
        steps
            .into_iter()
            .filter(|step| !step.has_line(Text("CARGO_PROFILE_DEV_CODEGEN_BACKEND: llvm")))
            .map(|step| {
                format!(
                    "{}:{}: coverage must set CARGO_PROFILE_DEV_CODEGEN_BACKEND to llvm",
                    self.name,
                    step.index + 1
                )
            })
            .collect()
    }

    /// Returns every missing common coverage execution field from every coverage step.
    pub fn coverage_execution_problems(&self) -> Problems {
        let steps = self.steps(Action::Coverage);
        if steps.is_empty() {
            return vec![format!(
                "{}: no generate-coverage step, so the check proves nothing",
                self.name
            )];
        }
        let required = std::iter::once(format!("uses: {COVERAGE_ACTION}"))
            .chain(
                COVERAGE_ENVIRONMENT
                    .iter()
                    .chain(COVERAGE_INPUTS)
                    .map(|field| (*field).to_owned()),
            )
            .collect::<Vec<_>>();
        let mut problems = Vec::new();
        for step in steps {
            for field in required.iter().filter(|field| !step.has_line(Text(field))) {
                problems.push(format!(
                    "{}:{}: coverage must retain {field}",
                    self.name,
                    step.index + 1
                ));
            }
        }
        problems
    }

    /// Returns one coverage-step `RUSTFLAGS` violation, if the step has one.
    fn coverage_rustflags_problem(&self, step: &Step<'_>) -> Option<String> {
        match step.rustflags() {
            None => Some(format!(
                "{}:{}: a coverage step does not assign RUSTFLAGS",
                self.name,
                step.index + 1
            )),
            Some(value) if value.contains("-Zthreads") || value.contains("mold") => Some(format!(
                "{}:{}: a coverage step assigns a standard flag: {value}",
                self.name,
                step.index + 1
            )),
            Some(_) => None,
        }
    }

    /// Returns coverage complaints for every coverage step in the workflow.
    pub fn coverage_problems(&self) -> Problems {
        self.steps(Action::Coverage)
            .into_iter()
            .filter_map(|step| self.coverage_rustflags_problem(&step))
            .collect()
    }

    /// Returns complaints about strict Whitaker setup and its ordering.
    fn strict_whitaker_step_problems(&self, step: &Step<'_>, lint_at: Option<usize>) -> Problems {
        let mut problems = Vec::new();
        if !step.has_compact(Text("cranelift:true")) {
            problems.push(format!(
                "{}: the strict Whitaker installer must pass cranelift: 'true'",
                self.name
            ));
        }
        if step.is_hidden() {
            problems.push(format!(
                "{}: the strict Whitaker installer must not be conditional or soft-failing",
                self.name
            ));
        }
        match lint_at {
            Some(position) if step.index < position => {}
            Some(_) => problems.push(format!(
                "{}: the strict Whitaker installer must precede make lint",
                self.name
            )),
            None => problems.push(format!(
                "{}: no reachable make lint step, so Whitaker ordering is unproved",
                self.name
            )),
        }
        problems
    }

    /// Returns the complaints about this workflow's Whitaker provisioner.
    pub fn whitaker_provisioning_problems(&self) -> Problems {
        let approved_action = format!("uses: {INSTALL_WHITAKER_ACTION}");
        let lines = WorkflowLines::new(Text(self.content));
        let lint_at = lines
            .steps_with_text(Text("run: make lint"))
            .first()
            .map(|step| step.index);
        let installer = lines
            .steps(Action::Whitaker)
            .into_iter()
            .find(|step| step.has_line(Text(&approved_action)));
        let mut problems = installer.map_or_else(
            || {
                vec![format!(
                    "{}: missing approved strict Whitaker installer action",
                    self.name
                )]
            },
            |step| self.strict_whitaker_step_problems(&step, lint_at),
        );
        if self
            .content
            .lines()
            .any(|line| !line.trim_start().starts_with('#') && line.contains("whitaker-installer"))
        {
            problems.push(format!("{}: must not install Whitaker directly", self.name));
        }
        if self
            .content
            .lines()
            .any(|line| line.trim_start().starts_with("suite-version:"))
        {
            problems.push(format!(
                "{}: must not override Whitaker suite selection",
                self.name
            ));
        }
        problems
    }

    /// Returns every setup and coverage complaint for one development workflow.
    ///
    /// For example, a direct `run: make test` workflow without `setup-rust`
    /// receives a missing-setup complaint.
    pub fn build_problems(&self) -> Problems {
        let has_coverage = !self.steps(Action::Coverage).is_empty();
        let mut problems = self.coverage_problems();
        if has_coverage {
            problems.extend(self.coverage_backend_problems());
            problems.extend(self.coverage_execution_problems());
        }
        problems.extend(
            WorkflowLines::new(Text(self.content)).development_problems(&Provision {
                setup_action_prefix: Text(SETUP_RUST_ACTION_PREFIX),
                workflow_name: Text(self.name),
            }),
        );
        problems
    }
}

/// Reads every supported workflow from the repository rather than preserving a
/// hand-maintained subset. A new workflow must therefore be parsed by the
/// contracts before it can silently become a build route.
///
/// # Errors
///
/// Returns an error when the workflow directory is unavailable, empty, or
/// contains a non-YAML file or an empty workflow. Those cases make the corpus
/// incomplete or ambiguous, so the contracts fail closed.
pub fn workflow_corpus() -> Result<Vec<(String, String)>, String> {
    let root = Dir::open_ambient_dir(env!("CARGO_MANIFEST_DIR"), ambient_authority())
        .map_err(|error| format!("opening repository root: {error}"))?;
    let directory = root
        .open_dir(".github/workflows")
        .map_err(|error| format!("opening .github/workflows: {error}"))?;
    let entries = directory
        .read_dir(".")
        .map_err(|error| format!("reading .github/workflows: {error}"))?;
    let mut workflows = Vec::new();
    for entry_result in entries {
        let entry = entry_result.map_err(|error| format!("reading workflow entry: {error}"))?;
        let name = entry
            .file_name()
            .map_err(|error| format!("reading workflow filename: {error}"))?;
        if !entry
            .file_type()
            .map_err(|error| format!("reading {name}: {error}"))?
            .is_file()
        {
            return Err(format!("unsupported workflow entry: {name}"));
        }
        if !matches!(
            name.rsplit_once('.').map(|(_, extension)| extension),
            Some("yml" | "yaml")
        ) {
            return Err(format!("unsupported workflow file: {name}"));
        }
        let text = directory
            .read_to_string(&name)
            .map_err(|error| format!("reading {name}: {error}"))?;
        if text.trim().is_empty() {
            return Err(format!("empty workflow file: {name}"));
        }
        workflows.push((name, text));
    }
    workflows.sort_by(|left, right| left.0.cmp(&right.0));
    if workflows.is_empty() {
        return Err("no workflows found in .github/workflows".to_owned());
    }
    Ok(workflows)
}

/// Returns every complaint about the discovered development workflows.
///
/// # Errors
///
/// Returns the corpus-discovery failure so a test cannot treat an unread
/// workflow as a compliant route.
pub fn workflow_problems() -> Result<Problems, String> {
    Ok(workflow_corpus()?
        .iter()
        .flat_map(|(name, text)| Workflow::new(name, text).build_problems())
        .collect())
}
