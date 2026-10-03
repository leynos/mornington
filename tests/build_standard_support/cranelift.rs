//! Reader for Cargo's actual development-profile Cranelift activation path.

/// The Cargo configuration whose development profile selects the backend.
pub const CONFIG: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/.cargo/config.toml"));
/// The pinned Rust toolchain that must supply the selected backend component.
pub const TOOLCHAIN: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/rust-toolchain.toml"));
/// The rustup component that provides Cargo's selected Cranelift backend.
const CRANELIFT_COMPONENT: &str = "rustc-codegen-cranelift-preview";

/// Returns the value assigned to a key in one exact TOML table. The build
/// standard supports a single value for each activation key, because duplicate
/// values would make the effective Cargo setting ambiguous.
fn table_value(config: &str, table: &str, key: &str) -> Result<Option<String>, String> {
    let mut current = "";
    let mut values = Vec::new();
    for line in config
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
    {
        if line.starts_with('[') && line.ends_with(']') {
            current = line.trim_matches(|character| matches!(character, '[' | ']'));
            continue;
        }
        if current == table {
            let Some(value) = line
                .strip_prefix(key)
                .map(str::trim_start)
                .and_then(|value| value.strip_prefix('='))
            else {
                continue;
            };
            values.push(value.trim().to_owned());
        }
    }
    match values.as_slice() {
        [] => Ok(None),
        [value] => Ok(Some(value.clone())),
        _ => Err(format!("[{table}] defines `{key}` more than once")),
    }
}

/// Returns every missing or malformed Cranelift activation prerequisite.
///
/// Cargo selects the backend through the `profile.dev` configuration, enabled
/// by its `unstable.codegen-backend` opt-in. Neither setting is a Rust flag, so
/// a `RUSTFLAGS` check would not prove the development default.
pub fn problems(config: &str, toolchain: &str) -> Result<Vec<String>, String> {
    let mut problems = Vec::new();
    let unstable = table_value(config, "unstable", "codegen-backend")?;
    if unstable.as_deref() != Some("true") {
        problems.push("[unstable] must enable codegen-backend".to_owned());
    }
    let development = table_value(config, "profile.dev", "codegen-backend")?;
    if development.as_deref() != Some("\"cranelift\"") {
        problems.push("[profile.dev] must select the Cranelift codegen backend".to_owned());
    }
    let component = format!("\"{CRANELIFT_COMPONENT}\"");
    if !toolchain
        .lines()
        .map(str::trim)
        .map(|line| line.trim_end_matches(','))
        .any(|line| line == component)
    {
        problems.push(format!(
            "rust-toolchain.toml must retain the {CRANELIFT_COMPONENT} component"
        ));
    }
    Ok(problems)
}
