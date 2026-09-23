//! Missing constructors prevent value resolution at compile time, not type lookup.
#![cfg(not(miri))]
#![forbid(unsafe_code)]

mod support;

use std::{fs, path::PathBuf, process::Command};

#[test]
fn type_only_registration_preserves_compile_time_resolution_requirements() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let target = root
        .join("target/type-only-contract")
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
    let provider = target.join("provider.rs");
    fs::write(
        &provider,
        r#"
#![no_std]
#![forbid(unsafe_code)]
pub trait IMessage {}
pub struct Message;
impl IMessage for Message {}
systasis::systasis_container! {
    register_type!(Message as IMessage);
}
"#,
    )
    .unwrap();
    let output = artifacts
        .rustc()
        .args([
            "--edition=2024",
            "--crate-type=rlib",
            "--crate-name=message_provider",
            "--out-dir",
        ])
        .arg(&target)
        .arg(&provider)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let external = format!(
        "message_provider={}",
        target.join("libmessage_provider.rlib").display()
    );

    let cases = [
        (
            "downstream_type_lookup",
            r#"
use core::marker::PhantomData;
#[systasis::container]
fn verify(primary: &message_provider::SystasisContainer) {
    let Ok(container) = systasis::systasis_container! {
        register_container!(primary: &message_provider::SystasisContainer);
        register_value!(PhantomData: PhantomData<resolve_type_from!(IMessage, primary)> as Copy);
    }.build();
    let _: PhantomData<message_provider::Message> = container.resolve_copy();
}
fn main() {
    let Ok(container) = message_provider::SystasisContainer::build();
    verify(&container);
}
"#,
            None,
        ),
        (
            "no_value_resolver",
            r#"
fn main() {
    let Ok(container) = message_provider::SystasisContainer::build();
    container.resolve_i_message();
}
"#,
            Some(("E0599", "Default")),
        ),
        (
            "no_generic_value_resolver",
            r#"
#[systasis::container]
fn verify<T: message_provider::IMessage>() {
    let Ok(container) = systasis::systasis_container! {
        register_type!(T as message_provider::IMessage);
    }.build();
    container.resolve_i_message();
}
fn main() {}
"#,
            Some(("E0599", "Default")),
        ),
        (
            "no_initializer_value_query",
            r#"
use message_provider::{Message, IMessage};
#[systasis::container]
fn main() {
    let Ok(_container) = systasis::systasis_container! {
        register_type!(Message as IMessage);
        register_value!(resolve!(IMessage): Message as IMessage in stored);
    }.build();
}
"#,
            Some(("E0599", "Default")),
        ),
        (
            "interface_still_required",
            r#"
struct WrongType;
#[systasis::container]
fn main() {
    let Ok(_container) = systasis::systasis_container! {
        register_type!(WrongType as message_provider::IMessage);
    }.build();
}
"#,
            Some(("E0277", "IMessage")),
        ),
    ];
    for (name, source, expected) in cases {
        let path = target.join(format!("{name}.rs"));
        fs::write(&path, source).unwrap();
        let output = artifacts
            .rustc()
            .args(["--edition=2024", "--extern", &external, "--out-dir"])
            .arg(&target)
            .arg(&path)
            .output()
            .unwrap();
        fs::write(target.join(format!("{name}.stderr")), &output.stderr).unwrap();
        let diagnostics = String::from_utf8_lossy(&output.stderr);
        if let Some((code, reason)) = expected {
            assert!(!output.status.success(), "{name} unexpectedly compiled");
            assert!(
                diagnostics.contains(&format!("error[{code}]")) && diagnostics.contains(reason),
                "{name}: {diagnostics}"
            );
        } else {
            assert!(output.status.success(), "{name}: {diagnostics}");
            assert!(Command::new(target.join(name)).status().unwrap().success());
        }
    }
}
