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

    if patterns.contains(&broad_pattern) {
        return Err(
            "the linker spelling exception must not accept the word outside an external-tool \
             context"
                .to_owned(),
        );
    }
    for required_pattern in required_patterns {
        if !patterns.contains(&required_pattern) {
            return Err(format!(
                "the local spelling overlay must retain the scoped external-linker pattern \
                 {required_pattern}"
            ));
        }
    }
    for pattern in [r"\bCARGO_TERM_COLOR\b", r"\bPolonius\b"] {
        if !patterns
            .iter()
            .any(|candidate| candidate.as_str() == pattern)
        {
            return Err(format!(
                "the local spelling overlay must retain the bounded external-name pattern \
                 {pattern}"
            ));
        }
    }
    Ok(())
}

fn local_ignore_patterns() -> Result<Vec<String>, String> {
    pattern_entries(TYPOS_LOCAL, "[patterns]\nignore = [", '\'')
        .map(|patterns| patterns.into_iter().map(str::to_owned).collect())
}

fn generated_ignore_patterns() -> Result<Vec<String>, String> {
    pattern_entries(TYPOS_GENERATED, "extend-ignore-re = [", '"')?
        .into_iter()
        .map(unescape_generated_pattern)
        .collect()
}

fn pattern_entries(
    config: &'static str,
    header: &str,
    delimiter: char,
) -> Result<Vec<&'static str>, String> {
    let Some((_, array)) = config.split_once(header) else {
        return Err(format!("the spelling configuration lacks {header:?}"));
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
            "the spelling configuration has no patterns after {header:?}"
        ));
    }
    Ok(patterns)
}

fn unescape_generated_pattern(pattern: &str) -> Result<String, String> {
    let mut escaped = false;
    let mut unescaped = String::new();
    for character in pattern.chars() {
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
    assert_linker_context_examples(&linker_name, &generated_patterns)
}

fn assert_linker_context_examples(linker_name: &str, patterns: &[String]) -> Result<(), String> {
    for accepted in [
        format!("-fuse-ld={linker_name}"),
        format!("install-{linker_name}"),
        format!("nightly pin, and {linker_name} on Linux"),
        format!("{linker_name} ${{expected}} is required"),
        format!("{linker_name} ${{version}} has no approved archive"),
    ] {
        if !matches_any_pattern(patterns, &accepted)? {
            return Err(format!(
                "the generated spelling patterns must recognize the external-linker context \
                 {accepted}"
            ));
        }
    }
    for ordinary_prose in [
        format!("The {linker_name} is unwelcome."),
        format!("{linker_name} on Linux"),
        format!("{linker_name} on the wall"),
        format!("install {linker_name} on the furniture"),
    ] {
        if matches_any_pattern(patterns, &ordinary_prose)? {
            return Err(format!(
                "the generated spelling patterns must leave ordinary prose detectable: \
                 {ordinary_prose}"
            ));
        }
    }
    Ok(())
}

fn matches_any_pattern(patterns: &[String], value: &str) -> Result<bool, String> {
    for pattern in patterns {
        let regex = Regex::new(pattern).map_err(|error| {
            format!("the configured spelling pattern {pattern:?} is invalid: {error}")
        })?;
        if regex.is_match(value) {
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
