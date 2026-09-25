//! Contract for the `CodeScene` publisher and the pull-request lane beside it.
//!
//! `coverage-main.yml` is the only `CodeScene` caller. Its runs queue on one
//! group per ref, its job runs in the main-only `codescene` environment that
//! holds the token, it uploads with `mode: upload` and no checksum input, and
//! it measures with the same `generate-coverage` revision as the pull-request
//! lane, which ratchets against its baseline and names no `CodeScene` surface.
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

/// The pull-request workflow, as committed.
const PULL_REQUEST_LANE: &str = include_str!("../.github/workflows/ci.yml");

/// The publisher's concurrency block: one group per ref, never cancelled.
const PUBLISHER_QUEUE: [&str; 3] = [
    "concurrency:",
    "group: coverage-main-${{ github.ref }}",
    "cancel-in-progress: false",
];

/// Returns whether `uses` pins its action to a full 40-character commit SHA.
///
/// Shape rather than value: Dependabot bumps these pins, and a mutable ref
/// such as `@main` would let both lanes agree while measuring anything.
fn is_sha_pinned(uses: &str) -> bool {
    uses.rsplit_once('@').is_some_and(|(_, reference)| {
        reference.len() == 40
            && reference
                .bytes()
                .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
    })
}

/// Returns the `uses:` line of the one step in `workflow` calling `action`.
fn action_line<'workflow>(workflow: &'workflow str, action: &str) -> &'workflow str {
    let marker = format!("/.github/actions/{action}@");
    let found: Vec<&str> = configuration_lines(workflow)
        .into_iter()
        .filter(|line| line.contains(&marker))
        .collect();
    assert_eq!(found.len(), 1, "expected one {action} step: {found:?}");
    let line = found.first().copied().unwrap_or_default();
    assert!(
        is_sha_pinned(line),
        "expected a full commit SHA pin: {line}"
    );
    line
}

/// Returns the `uses:` line of the one shared coverage step in `workflow`.
fn coverage_action(workflow: &str) -> &str { action_line(workflow, "generate-coverage") }

#[test]
fn the_publisher_queues_on_one_group_per_ref() {
    let lines = configuration_lines(PUBLISHER);
    let queue = lines
        .windows(PUBLISHER_QUEUE.len())
        .filter(|window| *window == PUBLISHER_QUEUE)
        .count();
    assert_eq!(
        queue, 1,
        "the publisher must queue on one group per ref and never cancel: {PUBLISHER_QUEUE:?}"
    );
}

#[test]
fn the_publisher_job_runs_in_the_codescene_environment() {
    let lines = configuration_lines(PUBLISHER);
    let declared = lines
        .iter()
        .position(|line| *line == "environment: codescene");
    let steps = lines.iter().position(|line| *line == "steps:");
    assert!(
        declared.is_some() && declared < steps,
        "the publisher job must declare `environment: codescene`, whose secret the token is"
    );
}

#[test]
fn the_upload_calls_the_pinned_shared_uploader() {
    let body = step_lines(PUBLISHER, "Upload coverage data to CodeScene");
    let uploader = action_line(PUBLISHER, "upload-codescene-coverage");
    assert!(
        uploader
            .starts_with("uses: leynos/shared-actions/.github/actions/upload-codescene-coverage@"),
        "the upload must call the shared uploader: {uploader}"
    );
    assert!(
        body.contains(&uploader),
        "the pinned uploader must be the upload step's own action"
    );
}

#[test]
fn the_upload_names_its_mode_and_passes_no_checksum() {
    let body = step_lines(PUBLISHER, "Upload coverage data to CodeScene");
    assert!(
        body.contains(&"mode: upload"),
        "the upload must name `mode: upload`"
    );
    assert!(
        !body
            .iter()
            .any(|line| line.starts_with("installer-checksum:")),
        "the uploader rejects a non-empty `installer-checksum`"
    );
}

#[test]
fn both_lanes_measure_with_one_coverage_revision() {
    assert_eq!(
        coverage_action(PUBLISHER),
        coverage_action(PULL_REQUEST_LANE),
        "the pull-request ratchet must read a baseline measured by the same action"
    );
    let lane = configuration_lines(PULL_REQUEST_LANE);
    for input in ["with-ratchet: 'true'", "publish-artefact: 'false'"] {
        assert!(
            lane.contains(&input),
            "the pull-request lane must set {input}"
        );
    }
}

#[test]
fn the_pull_request_lane_names_no_codescene_surface() {
    let text = PULL_REQUEST_LANE.to_ascii_lowercase();
    let reaching: Vec<&str> = ["cs_access_token", "codescene.io", "cs-coverage"]
        .into_iter()
        .filter(|marker| text.contains(marker))
        .collect();
    assert!(
        reaching.is_empty(),
        "ci.yml must name no CodeScene token, host or command, even in a comment: {reaching:?}"
    );
}
