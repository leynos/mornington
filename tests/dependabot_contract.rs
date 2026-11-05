//! Contracts for this repository's Dependabot update configuration.
//!
//! The configuration is parsed, so the assertions hold for any formatting.

use std::fmt;

use serde::{
    Deserialize,
    Deserializer,
    de::{MapAccess, Visitor},
};

/// The Dependabot configuration under contract.
const DEPENDABOT: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/.github/dependabot.yml"
));
/// The root Rust manifest that the Cargo entry must cover.
const CARGO_MANIFEST: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml"));

/// The parsed `dependabot.yml`.
#[derive(Deserialize)]
struct Config {
    /// One entry per owned ecosystem.
    updates: Vec<Update>,
}

/// One `updates` entry.
#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
struct Update {
    /// The ecosystem this entry updates.
    package_ecosystem: String,
    /// The directory the entry covers.
    directory: String,
    /// The update cadence.
    schedule: Schedule,
    /// The labels applied to the entry's pull requests.
    labels: Vec<String>,
    /// The groups, in the order Dependabot applies them.
    groups: Groups,
}

/// The update cadence.
#[derive(Deserialize)]
struct Schedule {
    /// How often Dependabot runs.
    interval: String,
}

/// One Dependabot group.
#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
struct Group {
    /// The dependency name patterns the group matches.
    patterns: Vec<String>,
    /// The update types the group accepts; empty means all.
    #[serde(default)]
    update_types: Vec<String>,
}

/// Groups keyed by name, keeping the file's order because order is precedence.
struct Groups(Vec<(String, Group)>);

/// Collects mapping entries in document order.
struct Ordered;

impl<'de> Visitor<'de> for Ordered {
    type Value = Groups;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a mapping of group names to groups")
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Groups, A::Error> {
        let mut groups = Vec::new();
        while let Some(entry) = map.next_entry::<String, Group>()? {
            groups.push(entry);
        }
        Ok(Groups(groups))
    }
}

impl<'de> Deserialize<'de> for Groups {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_map(Ordered)
    }
}

/// Parses the Dependabot configuration.
fn config() -> Result<Config, String> {
    serde_saphyr::from_str(DEPENDABOT).map_err(|error| format!("dependabot.yml: {error}"))
}

/// Every owned ecosystem is daily, rooted, and ends with the permitted catch-all.
#[test]
fn dependabot_entries_cover_the_root_with_daily_minor_and_patch_updates() -> Result<(), String> {
    if !CARGO_MANIFEST.contains("[package]") {
        return Err("this contract applies only while Cargo.toml remains the Rust root".to_owned());
    }
    let config = config()?;
    for ecosystem in ["github-actions", "cargo", "rust-toolchain"] {
        let Some(configured) = config
            .updates
            .iter()
            .find(|update| update.package_ecosystem == ecosystem)
        else {
            return Err(format!("Dependabot has no {ecosystem} entry"));
        };
        if configured.directory != "/" {
            return Err(format!(
                "Dependabot {ecosystem} must cover the repository root"
            ));
        }
        if configured.schedule.interval != "daily" {
            return Err(format!("Dependabot {ecosystem} must run daily"));
        }
        let expected_labels = ["dependencies", ecosystem];
        if configured.labels != expected_labels {
            return Err(format!(
                "Dependabot {ecosystem} must retain its canonical labels"
            ));
        }
        let Some((name, catch_all)) = configured.groups.0.last() else {
            return Err(format!("Dependabot {ecosystem} has no groups"));
        };
        if name != "minor-and-patch" {
            return Err(format!(
                "Dependabot {ecosystem} must end with the minor-and-patch catch-all"
            ));
        }
        if catch_all.patterns != ["*"] || catch_all.update_types != ["minor", "patch"] {
            return Err(format!(
                "Dependabot {ecosystem} must keep the catch-all on minor and patch updates"
            ));
        }
    }
    Ok(())
}
