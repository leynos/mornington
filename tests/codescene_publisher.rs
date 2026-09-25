//! Contract for where the `CodeScene` token may appear in the publisher.
//!
//! The upload is `upload-codescene-coverage`, a composite action whose nested
//! steps inherit the calling step's `env`, so `coverage-main.yml` binds the
//! token in no `env` at all. A check step publishes only whether the token
//! exists, the upload's guard reads that output beside the main-ref guard,
//! and the upload takes the token directly as its `access-token` input.
//!
//! The positive half matters as much as the prohibition: deleting the token
//! keeps every `env` clean while the upload skips forever, so the token must
//! be named on exactly two workflow lines, the check's command and the
//! upload's input. Comment lines are prose, not configuration, and are not
//! read.

/// The publisher workflow, as committed.
const PUBLISHER: &str = include_str!("../.github/workflows/coverage-main.yml");

/// The check step's one command line. GitHub evaluates the expression before
/// it sends the command to the runner, so the shell receives only `true` or
/// `false`.
const CHECK_RUN: &str =
    r#"run: echo "available=${{ secrets.CS_ACCESS_TOKEN != '' }}" >> "$GITHUB_OUTPUT""#;

/// The upload's guard: the check's output and the main ref, and nothing that
/// could make either optional.
const UPLOAD_GUARD: &str =
    "if: steps.codescene_token.outputs.available == 'true' && github.ref == 'refs/heads/main'";

/// The upload's input, reading the secret itself rather than an environment.
const UPLOAD_INPUT: &str = "access-token: ${{ secrets.CS_ACCESS_TOKEN }}";

/// Returns the workflow's configuration lines, trimmed, without comments.
fn configuration_lines(workflow: &str) -> Vec<&str> {
    workflow
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .collect()
}

/// Returns the trimmed body lines of the step whose name is `name`.
fn step_lines<'workflow>(workflow: &'workflow str, name: &str) -> Vec<&'workflow str> {
    let lines = configuration_lines(workflow);
    let header = format!("- name: {name}");
    let starts: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter(|(_, line)| **line == header)
        .map(|(index, _)| index)
        .collect();
    assert_eq!(
        starts.len(),
        1,
        "the publisher must declare one {name:?} step"
    );
    let start = starts.first().copied().unwrap_or_default();
    lines
        .iter()
        .skip(start + 1)
        .take_while(|line| !line.starts_with("- "))
        .copied()
        .collect()
}

/// Returns the index of the step called `name` among the step headers.
fn step_position(workflow: &str, name: &str) -> Option<usize> {
    configuration_lines(workflow)
        .iter()
        .filter(|line| line.starts_with("- "))
        .position(|line| *line == format!("- name: {name}"))
}

#[test]
fn the_check_step_publishes_availability_and_nothing_else() {
    let body = step_lines(PUBLISHER, "Check CodeScene token availability");
    assert_eq!(
        body,
        vec!["id: codescene_token", CHECK_RUN],
        "the check must carry its id and its one command, with no `if:` and no `env`"
    );
    assert!(
        step_position(PUBLISHER, "Check CodeScene token availability")
            < step_position(PUBLISHER, "Upload coverage data to CodeScene"),
        "the check must run before the upload that reads its output"
    );
}

#[test]
fn the_upload_reads_the_check_and_the_ref_and_takes_the_token() {
    let body = step_lines(PUBLISHER, "Upload coverage data to CodeScene");
    assert!(
        body.contains(&UPLOAD_GUARD),
        "the upload must be guarded exactly by {UPLOAD_GUARD:?}, got {body:?}"
    );
    assert!(
        body.contains(&UPLOAD_INPUT),
        "the upload must pass {UPLOAD_INPUT:?}, got {body:?}"
    );
    assert!(
        !body.contains(&"env:"),
        "the upload must declare no `env`; its nested steps would inherit it"
    );
}

#[test]
fn the_token_appears_exactly_where_it_is_used() {
    let mentions: Vec<&str> = configuration_lines(PUBLISHER)
        .into_iter()
        .filter(|line| line.to_ascii_lowercase().contains("cs_access_token"))
        .collect();
    assert_eq!(
        mentions,
        vec![CHECK_RUN, UPLOAD_INPUT],
        "the token may be named only by the check's command and the upload's input"
    );
}
