//! Contracts for this repository's Dependabot update configuration.

/// The Dependabot configuration under contract.
const DEPENDABOT: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/.github/dependabot.yml"
));
/// The root Rust manifest that the Cargo entry must cover.
const CARGO_MANIFEST: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml"));

/// Returns one ecosystem entry, ending before the next Dependabot entry.
fn entry(ecosystem: &str) -> Result<&'static str, String> {
    let marker = format!("  - package-ecosystem: \"{ecosystem}\"\n");
    let Some((_, remaining)) = DEPENDABOT.split_once(&marker) else {
        return Err(format!("Dependabot has no {ecosystem} entry"));
    };
    Ok(remaining
        .split("\n  - package-ecosystem:")
        .next()
        .unwrap_or_default())
}

/// Returns the group names in the order Dependabot applies them.
fn group_names(entry: &str) -> Result<Vec<&str>, String> {
    let Some((_, groups)) = entry.split_once("    groups:\n") else {
        return Err("Dependabot entry has no groups mapping".to_owned());
    };
    Ok(groups
        .lines()
        .filter_map(|line| line.strip_prefix("      "))
        .filter(|line| !line.starts_with(' '))
        .filter_map(|line| line.strip_suffix(':'))
        .collect())
}

/// Every owned ecosystem is daily, rooted, and ends with the permitted catch-all.
#[test]
fn dependabot_entries_cover_the_root_with_daily_minor_and_patch_updates() -> Result<(), String> {
    if !CARGO_MANIFEST.contains("[package]") {
        return Err("this contract applies only while Cargo.toml remains the Rust root".to_owned());
    }
    for ecosystem in ["github-actions", "cargo"] {
        let configured = entry(ecosystem)?;
        if !configured.contains("    directory: \"/\"\n") {
            return Err(format!(
                "Dependabot {ecosystem} must cover the repository root"
            ));
        }
        if !configured.contains("    schedule:\n      interval: \"daily\"\n") {
            return Err(format!("Dependabot {ecosystem} must run daily"));
        }
        let expected_labels =
            format!("    labels:\n      - \"dependencies\"\n      - \"{ecosystem}\"\n");
        if !configured.contains(&expected_labels) {
            return Err(format!(
                "Dependabot {ecosystem} must retain its canonical labels"
            ));
        }
        let groups = group_names(configured)?;
        if groups.last() != Some(&"minor-and-patch") {
            return Err(format!(
                "Dependabot {ecosystem} must end with the minor-and-patch catch-all"
            ));
        }
        let expected = concat!(
            "      minor-and-patch:\n",
            "        patterns:\n",
            "          - \"*\"\n",
            "        update-types:\n",
            "          - \"minor\"\n",
            "          - \"patch\"\n"
        );
        if !configured.contains(expected) {
            return Err(format!(
                "Dependabot {ecosystem} must keep the minor-and-patch catch-all exact"
            ));
        }
    }
    Ok(())
}
