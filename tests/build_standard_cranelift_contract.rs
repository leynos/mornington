//! Contracts for Cargo's development-profile Cranelift activation path.

#[path = "build_standard_support/cranelift.rs"]
mod cranelift;

use cranelift::{CONFIG, TOOLCHAIN, problems};

/// The real Cargo configuration enables and selects Cranelift, while the pinned
/// toolchain supplies the selected backend component.
#[test]
fn development_builds_activate_the_pinned_cranelift_backend() -> Result<(), String> {
    let problems = problems(CONFIG, TOOLCHAIN)?;
    if problems.is_empty() {
        Ok(())
    } else {
        Err(format!("Cranelift activation drifted: {problems:#?}"))
    }
}

/// Removing any part of the actual activation path must fail, rather than being
/// masked by unrelated `RUSTFLAGS` contracts.
#[test]
fn cranelift_activation_mutations_are_refused() -> Result<(), String> {
    let disabled_opt_in = CONFIG.replacen("codegen-backend = true\n", "", 1);
    let removed_development_default = CONFIG.replacen("codegen-backend = \"cranelift\"\n", "", 1);
    let missing_component = TOOLCHAIN.replacen("  \"rustc-codegen-cranelift-preview\",\n", "", 1);
    for (description, config, toolchain) in [
        (
            "disabled Cargo codegen-backend opt-in",
            disabled_opt_in,
            TOOLCHAIN.to_owned(),
        ),
        (
            "removed development Cranelift default",
            removed_development_default,
            TOOLCHAIN.to_owned(),
        ),
        (
            "removed rustup Cranelift component",
            CONFIG.to_owned(),
            missing_component,
        ),
    ] {
        if problems(&config, &toolchain)?.is_empty() {
            return Err(format!("the {description} mutation passed"));
        }
    }
    Ok(())
}
