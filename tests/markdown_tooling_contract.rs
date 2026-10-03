//! Contracts for direct Markdown formatting and canonical lint policy wiring.

use regex::Regex;

/// The generated target definitions under contract.
const MAKEFILE: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/Makefile"));
/// The canonical markdownlint policy configuration.
const MARKDOWNLINT: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/.markdownlint-cli2.jsonc"
));
/// The repository-specific spelling overlay.
const TYPOS_LOCAL: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/typos.local.toml"));
/// The spelling configuration regenerated from the shared dictionary and overlay.
const TYPOS_GENERATED: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/typos.toml"));
/// The repository ignore rules for the shared spelling dictionary cache.
const GITIGNORE: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/.gitignore"));

/// A borrowed spelling-pattern collection.
#[derive(Clone, Copy)]
struct Patterns<'a>(&'a [String]);

/// One spelling pattern or example string.
#[derive(Clone, Copy)]
struct Text<'a>(&'a str);

/// A collection of spelling examples with the same expected outcome.
#[derive(Clone, Copy)]
struct Examples<'a>(&'a [String]);

/// Whether examples must match or remain visible to the spelling checker.
#[derive(Clone, Copy)]
enum MatchExpectation {
    Required,
    Rejected,
}

/// Formatting calls mdtablefix directly with the repository and wrapping rules.
#[test]
fn makefile_uses_direct_mdtablefix_contracts() {
    for required_fragment in [
        "MDTABLEFIX ?= mdtablefix",
        "MDTABLEFIX_SELECT = --git --include-untracked",
        "MDTABLEFIX_RULES = --wrap --renumber --breaks --ellipsis --fences",
        "$(MDTABLEFIX) --in-place $(MDTABLEFIX_SELECT) $(MDTABLEFIX_RULES)",
        "$(MDTABLEFIX) --check $(MDTABLEFIX_SELECT) $(MDTABLEFIX_RULES)",
        "$(MDLINT) --fix \"**/*.md\"",
    ] {
        assert!(
            MAKEFILE.contains(required_fragment),
            "the Markdown formatting contract must retain {required_fragment}"
        );
    }
}

/// The canonical rules distinguish tabs in code blocks from code-block width.
#[test]
fn markdownlint_keeps_the_canonical_code_block_rules() {
    for required_fragment in [
        "\"MD010\": { \"code_blocks\": false }",
        "\"code_block_line_length\": 120",
        "\"line_length\": 80",
        "\"tables\": false",
        "\"headings\": false",
    ] {
        assert!(
            MARKDOWNLINT.contains(required_fragment),
            "the canonical markdownlint policy must retain {required_fragment}"
        );
    }
}

/// Spelling uses the pinned all-repository builder gate and does not mask all inline code.
#[test]
fn spelling_uses_the_selected_builder_without_an_inline_code_exemption() -> Result<(), String> {
    assert_spelling_builder_gate();
    assert_bounded_spelling_overlay()?;
    assert_generated_spelling_config_is_not_stale()?;
    assert_spelling_cache_rules();
    Ok(())
}

fn assert_spelling_builder_gate() {
    assert!(
        MAKEFILE.contains("TYPOS_CONFIG_BUILDER_VERSION = v0.1.3"),
        "the spelling builder must remain pinned to v0.1.3"
    );
    let gate_commands: Vec<_> = MAKEFILE
        .lines()
        .filter(|line| line.contains("$(TYPOS_CONFIG_BUILDER) gate"))
        .collect();
    assert_eq!(
        gate_commands,
        ["\t$(TYPOS_CONFIG_BUILDER) gate --repository . --scope all"],
        "spelling must invoke the builder once with --scope all so source symbols are checked"
    );
}

fn assert_bounded_spelling_overlay() -> Result<(), String> {
    let linker_name = ["mo", "ld"].concat();
    let patterns = local_ignore_patterns()?;
    let broad_pattern = format!(r"\b{linker_name}\b");
    let required_patterns = [
        format!(r"(?:-fuse-ld=|install-|command -v |tools/)\b{linker_name}\b"),
        format!(r"\b{linker_name}(?:-(?:[0-9]|%s)|/[A-Za-z0-9_]| --version| \$\{{| [0-9])"),
    ];

    reject_pattern(Patterns(&patterns), Text(&broad_pattern))?;
    require_patterns(
        Patterns(&patterns),
        required_patterns.iter().map(|pattern| Text(pattern)),
        Text("scoped external-linker"),
    )?;
    require_patterns(
        Patterns(&patterns),
        [Text(r"\bCARGO_TERM_COLOR\b"), Text(r"\bPolonius\b")],
        Text("bounded external-name"),
    )?;
    Ok(())
}

/// Rejects one over-broad spelling pattern.
fn reject_pattern(patterns: Patterns<'_>, rejected: Text<'_>) -> Result<(), String> {
    if patterns.0.iter().any(|pattern| pattern == rejected.0) {
        Err(
            "the linker spelling exception must not accept the word outside an external-tool \
             context"
                .to_owned(),
        )
    } else {
        Ok(())
    }
}

/// Requires every selected spelling pattern to remain in the overlay.
fn require_patterns<'a>(
    patterns: Patterns<'_>,
    required: impl IntoIterator<Item = Text<'a>>,
    kind: Text<'_>,
) -> Result<(), String> {
    for pattern in required {
        if !patterns.0.iter().any(|candidate| candidate == pattern.0) {
            return Err(format!(
                "the local spelling overlay must retain the {} pattern {}",
                kind.0, pattern.0
            ));
        }
    }
    Ok(())
}

fn local_ignore_patterns() -> Result<Vec<String>, String> {
    pattern_entries(Text(TYPOS_LOCAL), Text("[patterns]\nignore = ["), '\'')
        .map(|patterns| patterns.into_iter().map(str::to_owned).collect())
}

fn generated_ignore_patterns() -> Result<Vec<String>, String> {
    pattern_entries(Text(TYPOS_GENERATED), Text("extend-ignore-re = ["), '"')?
        .into_iter()
        .map(|pattern| unescape_generated_pattern(Text(pattern)))
        .collect()
}

fn pattern_entries(
    config: Text<'static>,
    header: Text<'_>,
    delimiter: char,
) -> Result<Vec<&'static str>, String> {
    let Some((_, array)) = config.0.split_once(header.0) else {
        return Err(format!("the spelling configuration lacks {:?}", header.0));
    };
    let mut patterns = Vec::new();
    for line in array.lines().skip(1).take_while(|line| line.trim() != "]") {
        let line_entry = line.trim().trim_end_matches(',');
        if line_entry.is_empty() || line_entry.starts_with('#') {
            continue;
        }
        let Some(delimited_entry) = line_entry.strip_prefix(delimiter) else {
            return Err(format!(
                "the spelling configuration has an unquoted pattern {line_entry:?}"
            ));
        };
        let Some(pattern) = delimited_entry.strip_suffix(delimiter) else {
            return Err(format!(
                "the spelling configuration has an unterminated pattern {line_entry:?}"
            ));
        };
        patterns.push(pattern);
    }
    if patterns.is_empty() {
        return Err(format!(
            "the spelling configuration has no patterns after {header:?}",
            header = header.0
        ));
    }
    Ok(patterns)
}

fn unescape_generated_pattern(pattern: Text<'_>) -> Result<String, String> {
    let mut escaped = false;
    let mut unescaped = String::new();
    for character in pattern.0.chars() {
        if escaped {
            let replacement = match character {
                '\\' => '\\',
                '"' => '"',
                'n' => '\n',
                'r' => '\r',
                't' => '\t',
                _ => {
                    return Err(format!(
                        "the generated pattern has an unsupported escape \\{character}"
                    ));
                }
            };
            unescaped.push(replacement);
            escaped = false;
        } else if character == '\\' {
            escaped = true;
        } else {
            unescaped.push(character);
        }
    }
    if escaped {
        return Err("the generated pattern ends in an escape".to_owned());
    }
    Ok(unescaped)
}

fn assert_generated_spelling_config_is_not_stale() -> Result<(), String> {
    let linker_name = ["mo", "ld"].concat();
    let local_patterns = local_ignore_patterns()?;
    let generated_patterns = generated_ignore_patterns()?;

    for local_pattern in &local_patterns {
        if !generated_patterns.contains(local_pattern) {
            return Err(format!(
                "the generated spelling configuration must retain {local_pattern:?}"
            ));
        }
    }
    assert_linker_context_examples(Text(&linker_name), Patterns(&generated_patterns))
}

fn assert_linker_context_examples(
    linker_name: Text<'_>,
    patterns: Patterns<'_>,
) -> Result<(), String> {
    let name = linker_name.0;
    let accepted = [
        format!("-fuse-ld={name}"),
        format!("install-{name}"),
        format!("nightly pin, and {name} on Linux"),
        format!("{name} ${{expected}} is required"),
        format!("{name} ${{version}} has no approved archive"),
    ];
    assert_example_matches(patterns, Examples(&accepted), MatchExpectation::Required)?;
    let ordinary_prose = [
        format!("The {name} is unwelcome."),
        format!("{name} on Linux"),
        format!("{name} on the wall"),
        format!("install {name} on the furniture"),
    ];
    assert_example_matches(
        patterns,
        Examples(&ordinary_prose),
        MatchExpectation::Rejected,
    )
}

/// Applies one matching expectation to every supplied spelling example.
fn assert_example_matches(
    patterns: Patterns<'_>,
    examples: Examples<'_>,
    expectation: MatchExpectation,
) -> Result<(), String> {
    for example in examples.0 {
        let matches = matches_any_pattern(patterns, Text(example))?;
        if matches != matches!(expectation, MatchExpectation::Required) {
            return Err(expectation.failure(Text(example)));
        }
    }
    Ok(())
}

impl MatchExpectation {
    /// Returns the diagnostic for one example that violates this expectation.
    fn failure(self, example: Text<'_>) -> String {
        match self {
            Self::Required => format!(
                "the generated spelling patterns must recognize the external-linker context {}",
                example.0
            ),
            Self::Rejected => format!(
                "the generated spelling patterns must leave ordinary prose detectable: {}",
                example.0
            ),
        }
    }
}

fn matches_any_pattern(patterns: Patterns<'_>, value: Text<'_>) -> Result<bool, String> {
    for pattern in patterns.0 {
        let regex = Regex::new(pattern).map_err(|error| {
            format!("the configured spelling pattern {pattern:?} is invalid: {error}")
        })?;
        if regex.is_match(value.0) {
            return Ok(true);
        }
    }
    Ok(false)
}

fn assert_spelling_cache_rules() {
    for cache_file in [".typos-oxendict-base.json", ".typos-oxendict-base.toml"] {
        assert!(
            GITIGNORE.lines().any(|line| line == cache_file),
            "the shared dictionary cache must remain ignored: {cache_file}"
        );
    }
    assert!(
        !GITIGNORE
            .lines()
            .any(|line| line.trim_start().starts_with('!')),
        "the repository must not unignore the shared dictionary cache with a later pattern"
    );
    assert!(
        !GITIGNORE.lines().any(|line| line == "typos.toml"),
        "the generated spelling configuration must remain trackable"
    );
}
