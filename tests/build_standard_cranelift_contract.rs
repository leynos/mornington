//! Contracts for Cargo's development-profile Cranelift activation path.

#[path = "build_standard_support/cranelift.rs"]
mod cranelift;

use cranelift::{CONFIG, MANIFEST, TOOLCHAIN, manifest_problems, problems};

/// The root manifest leaves the development backend unset or on LLVM, and a
/// manifest that selects Cranelift is refused.
#[test]
fn the_manifest_does_not_select_cranelift() -> Result<(), String> {
    let real = manifest_problems(MANIFEST)?;
    if !real.is_empty() {
        return Err(format!("the manifest selects a backend: {real:#?}"));
    }
    let mutated = format!("{MANIFEST}\n[profile.dev]\ncodegen-backend = \"cranelift\"\n");
    if manifest_problems(&mutated)?.is_empty() {
        return Err("a manifest selecting Cranelift passed".to_owned());
    }
    Ok(())
}

/// The real Cargo configuration keeps the unstable key and leaves the
/// development profile off Cranelift, while the pinned toolchain keeps the
/// backend component for an opt-in.
#[test]
fn development_builds_keep_cranelift_available_but_unselected() -> Result<(), String> {
    let problems = problems(CONFIG, TOOLCHAIN)?;
    if problems.is_empty() {
        Ok(())
    } else {
        Err(format!("Cranelift activation drifted: {problems:#?}"))
    }
}

/// Selecting Cranelift for the development profile, or removing the opt-in key
/// or the component that keeps it available, must fail, rather than being
/// masked by unrelated `RUSTFLAGS` contracts.
#[test]
fn cranelift_activation_mutations_are_refused() -> Result<(), String> {
    let disabled_opt_in = CONFIG.replacen("codegen-backend = true\n", "", 1);
    let selected_development_default = CONFIG.replacen(
        "# codegen-backend = \"cranelift\"\n",
        "codegen-backend = \"cranelift\"\n",
        1,
    );
    let missing_component = TOOLCHAIN.replacen("  \"rustc-codegen-cranelift-preview\",\n", "", 1);
    for (description, config, toolchain) in [
        (
            "disabled Cargo codegen-backend opt-in",
            disabled_opt_in,
            TOOLCHAIN.to_owned(),
        ),
        (
            "selected development Cranelift default",
            selected_development_default,
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
