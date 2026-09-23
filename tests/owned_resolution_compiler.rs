//! Removed accessors are rejected instead of silently retaining lending or take APIs.
#![cfg(not(miri))]
#![forbid(unsafe_code)]
mod support;
use std::{fs, path::PathBuf, process::Command};

#[test]
fn resolver_method_sets_match_the_stored_value_policy() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let target = root
        .join("target/owned-resolution-contract")
        .join(if cfg!(feature = "std") {
            "std"
        } else {
            "no-std"
        });
    let mut build = Command::new(env!("CARGO"));
    build
        .current_dir(&root)
        .args(["build", "--lib", "--offline", "--locked", "--target-dir"])
        .arg(&target);
    if !cfg!(feature = "std") {
        build.arg("--no-default-features");
    }
    let artifacts = support::Artifacts::build(&mut build);
    for (name, statement, expected) in [
        (
            "copy_ref",
            "container.resolve_copy_ref();",
            "no method named `resolve_copy_ref`",
        ),
        (
            "copy_try_ref",
            "container.try_resolve_copy_ref();",
            "no method named `try_resolve_copy_ref`",
        ),
        (
            "clone_take",
            "container.try_resolve_i_text();",
            "no method named `try_resolve_i_text`",
        ),
        (
            "clone_plain",
            "container.resolve_i_text();",
            "no method named `resolve_i_text`",
        ),
        (
            "clone_ref_mut",
            "container.try_resolve_i_text_ref_mut();",
            "no method named `try_resolve_i_text_ref_mut`",
        ),
        (
            "move_plain",
            "container.resolve_i_owned();",
            "no method named `resolve_i_owned`",
        ),
        (
            "move_clone",
            "container.try_resolve_i_owned_clone();",
            "no method named `try_resolve_i_owned_clone`",
        ),
    ] {
        let source = format!(
            r#"
trait IText {{}}
impl IText for String {{}}
struct Owned;
trait IOwned {{}}
impl IOwned for Owned {{}}
#[systasis::container]
fn main() {{
    let Ok(container) = systasis::systasis_container! {{
        register_value!(7: u8 as Copy);
        register_value!(String::from("text"): String as IText);
        register_value!(Owned: Owned as IOwned);
    }}.build();
    {statement}
}}
"#
        );
        let path = target.join(format!("{name}.rs"));
        fs::write(&path, source).unwrap();
        let output = artifacts
            .rustc()
            .args(["--edition=2024", "--out-dir"])
            .arg(&target)
            .arg(&path)
            .output()
            .unwrap();
        let diagnostics = String::from_utf8_lossy(&output.stderr);
        assert!(!output.status.success(), "{name} unexpectedly compiled");
        assert!(diagnostics.contains(expected), "{name}: {diagnostics}");
    }
    for query in [
        "resolve_ref!(Copy)",
        "try_resolve_ref!(Copy)",
        "try_resolve_ref_mut!(Copy)",
    ] {
        let path = target.join("removed_query.rs");
        fs::write(
            &path,
            format!(
                r#"
#[systasis::container]
fn main() {{
    let Ok(container) = systasis::systasis_container! {{
        register_value!(7: u8 as Copy);
        register_value!({query}: u8 as Copy in output);
    }}.build();
}}
"#
            ),
        )
        .unwrap();
        let output = artifacts
            .rustc()
            .args(["--edition=2024", "--out-dir"])
            .arg(&target)
            .arg(&path)
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(
            String::from_utf8_lossy(&output.stderr)
                .contains("borrowed resolvers have been removed")
        );
    }
}
