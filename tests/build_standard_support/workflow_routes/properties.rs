//! Bounded generated contracts for runner normalization and reusable test routes.

use proptest::{
    prelude::*,
    test_runner::{Config, TestRunner},
};

use super::workflow_routes::suite_provisioning_problems;

/// A strict provision step whose place in the generated route is varied.
const PROVISION: &str = concat!(
    "      - uses: leynos/shared-actions/.github/actions/setup-rust@approved-ref\n",
    "        with:\n          install-mold: 'true'\n"
);

/// Runs 64 cases per invariant while retaining Proptest's shrinking and replay.
fn property_runner() -> TestRunner { TestRunner::new(Config::with_cases(64)) }

/// Wraps generated runner labels and steps in the actual public reader's input.
fn route_workflow(labels: &str, steps: &str, mapping: bool) -> String {
    let runner = if mapping {
        format!("runs-on:\n      labels: [{labels}]")
    } else {
        format!("runs-on: [{labels}]")
    };
    format!("jobs:\n  suite:\n    {runner}\n    steps:\n{steps}")
}

/// Whitespace, YAML quoting, ASCII case, metadata, and label ordering preserve
/// Linux route meaning; adding another OS is refused under every transformation.
#[test]
fn runner_normalization_and_label_order_preserve_fail_closed_meaning() -> Result<(), String> {
    let strategy = (
        any::<[u16; 4]>(),
        any::<bool>(),
        any::<bool>(),
        any::<bool>(),
        any::<bool>(),
        "fleet[0-9]{0,8}",
    );
    property_runner()
        .run(
            &strategy,
            |(priorities, quoted, uppercase, mapping, conflicting, metadata)| {
                let platform = if conflicting { "windows" } else { "x64" };
                let mut labels: Vec<_> = ["self-hosted", "linux", platform, metadata.as_str()]
                    .into_iter()
                    .zip(priorities)
                    .collect();
                labels.sort_by_key(|(_, priority)| *priority);
                let declaration = labels
                    .iter()
                    .map(|(label, _)| {
                        let spelling = if uppercase {
                            label.to_ascii_uppercase()
                        } else {
                            (*label).to_owned()
                        };
                        if quoted {
                            format!("  '{spelling}'  ")
                        } else {
                            format!("  {spelling}  ")
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(",");
                let workflow = route_workflow(
                    &declaration,
                    &format!("{PROVISION}      - run: make test\n"),
                    mapping,
                );
                let problems = suite_provisioning_problems(&[("fixture.yml", &workflow)]);
                if conflicting {
                    prop_assert_eq!(
                        problems.len(),
                        1,
                        "conflicting labels must fail: {:?}",
                        problems
                    );
                    prop_assert!(
                        problems
                            .iter()
                            .all(|problem| problem.contains("conflicting operating systems")),
                        "conflicts must be diagnosed as runner ambiguity: {:?}",
                        problems
                    );
                } else {
                    prop_assert!(
                        problems.is_empty(),
                        "normalization must preserve Linux provision: {:?}",
                        problems
                    );
                }
                Ok(())
            },
        )
        .map_err(|error| format!("runner normalization invariant failed: {error}"))
}

/// Moving the provision across a generated suite position changes reachability
/// exactly when the provision ceases to precede the first test command.
#[test]
fn provisioning_order_controls_suite_reachability() -> Result<(), String> {
    property_runner()
        .run(
            &(0usize..12, 0usize..12, 0usize..12),
            |(provision_at, suite_at, padding)| {
                let mut steps = Vec::new();
                for position in 0..12 {
                    if position == suite_at {
                        steps.push("      - run: cargo test --all-features\n".to_owned());
                    }
                    if position == provision_at {
                        steps.push(PROVISION.to_owned());
                    }
                    if position < padding {
                        steps.push("      - run: echo harmless\n".to_owned());
                    }
                }
                let workflow = route_workflow("ubuntu-latest", &steps.concat(), false);
                let problems = suite_provisioning_problems(&[("fixture.yml", &workflow)]);
                prop_assert_eq!(
                    problems.is_empty(),
                    provision_at < suite_at,
                    "provision at {} and suite at {} must determine route safety: {:?}",
                    provision_at,
                    suite_at,
                    problems
                );
                Ok(())
            },
        )
        .map_err(|error| format!("provision ordering invariant failed: {error}"))
}

/// Builds a bounded local reusable chain ending in either a suite or a back edge.
fn reusable_chain(
    length: usize,
    back_edge: Option<usize>,
    provisioned: bool,
) -> Vec<(String, String)> {
    (0..length)
        .map(|at| {
            let name = format!("node-{at}.yml");
            let target = if at + 1 < length {
                Some(at + 1)
            } else {
                back_edge
            };
            let workflow = target.map_or_else(
                || {
                    let setup = if provisioned { PROVISION } else { "" };
                    format!(
                        "jobs:\n  suite:\n    runs-on: ubuntu-latest\n    steps:\n{setup}      - \
                         run: make test\n"
                    )
                },
                |next| format!("jobs:\n  caller:\n    uses: ./.github/workflows/node-{next}.yml\n"),
            );
            (name, workflow)
        })
        .collect()
}

/// Every chain entry reaches the same terminal suite, while any back edge is a
/// cycle that must be refused instead of recursing indefinitely or appearing safe.
#[test]
fn reusable_chains_preserve_terminal_contract_and_cycles_fail_closed() -> Result<(), String> {
    let strategy = (1usize..9)
        .prop_flat_map(|length| (Just(length), any::<bool>(), any::<bool>(), 0usize..length));
    property_runner()
        .run(&strategy, |(length, cyclic, provisioned, edge)| {
            let back_edge = cyclic.then_some(edge);
            let corpus = reusable_chain(length, back_edge, provisioned);
            let references: Vec<_> = corpus
                .iter()
                .map(|(name, workflow)| (name.as_str(), workflow.as_str()))
                .collect();
            let problems = suite_provisioning_problems(&references);
            if cyclic {
                prop_assert!(
                    !problems.is_empty(),
                    "every generated cycle must fail: {:?}",
                    references
                );
                prop_assert!(
                    problems
                        .iter()
                        .all(|problem| problem.contains("call cycle")),
                    "cycle refusal must identify its reason: {:?}",
                    problems
                );
            } else {
                prop_assert_eq!(
                    problems.is_empty(),
                    provisioned,
                    "chain terminal provision must determine safety: {:?}",
                    problems
                );
                if !provisioned {
                    prop_assert_eq!(
                        problems.len(),
                        1,
                        "terminal diagnostics must deduplicate: {:?}",
                        problems
                    );
                }
            }
            Ok(())
        })
        .map_err(|error| format!("reusable graph invariant failed: {error}"))
}
