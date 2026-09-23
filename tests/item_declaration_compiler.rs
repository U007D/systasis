//! Cross-crate declaration naming, inferred errors and auto-trait checks.
#![forbid(unsafe_code)]
mod support;

use std::{fs, path::PathBuf, process::Command};

#[test]
fn cross_crate_inferred_names_and_current_boundaries() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let target = root
        .join("target/item-declaration-contract")
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
pub mod di {
    systasis::systasis_container! { register_value!(42: u8 as Copy); }
}
pub mod fallible {
    trait IFlag {}
    impl IFlag for bool {}
    systasis::systasis_container! {
        register_value!("invalid".parse::<u8>()?: u8 as Copy);
        register_value!("true".parse::<bool>()?: bool as IFlag);
    }
}
pub mod local_error {
    extern crate alloc;
    #[derive(Debug)]
    pub struct Failure(#[allow(dead_code)] alloc::rc::Rc<()>);
    impl core::fmt::Display for Failure {
        fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result { f.write_str("local failure") }
    }
    impl core::error::Error for Failure {}
    systasis::systasis_container! {
        register_value!(Err::<u8, Failure>(Failure(alloc::rc::Rc::new(())))?: u8 as Copy);
    }
}
"#,
    )
    .unwrap();
    let output = artifacts
        .rustc()
        .args([
            "--edition=2024",
            "--crate-type=rlib",
            "--crate-name=item_provider",
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
        "item_provider={}",
        target.join("libitem_provider.rlib").display()
    );
    let cases = [
        (
            "named",
            r#"
use item_provider::di::SystasisContainer;
fn init() -> SystasisContainer { let Ok(container) = SystasisContainer::build(); container }
fn main() { assert_eq!(init().resolve_copy(), 42); }
"#,
            None,
        ),
        (
            "inferred_fallible_name",
            r#"
use item_provider::fallible::{SystasisContainer, SystasisContainerError};
fn init() -> Result<SystasisContainer, SystasisContainerError> { SystasisContainer::build() }
fn main() {
    let Err(error) = init() else { panic!("parse fails") };
    fn send_sync<T: Send + Sync>(_: &T) {}
    send_sync(&error);
    assert!(core::error::Error::source(&error).unwrap().is::<core::num::ParseIntError>());
}
"#,
            None,
        ),
        (
            "non_send_error_remains_local",
            r#"
use item_provider::local_error::SystasisContainer;
fn main() {
    let Err(error) = SystasisContainer::build() else { panic!("fails") };
    assert_eq!(error.to_string(), "local failure");
}
"#,
            None,
        ),
        (
            "non_send_error_rejected",
            r#"
use item_provider::local_error::SystasisContainer;
fn main() {
    let Err(error) = SystasisContainer::build() else { panic!("fails") };
    fn send<T: Send>(_: T) {}
    send(error);
}
"#,
            Some("cannot be sent between threads safely"),
        ),
        (
            "non_error_source_boundary",
            r#"
struct Failure;
systasis::systasis_container! { register_value!(Err::<u8, Failure>(Failure)?: u8 as Copy); }
fn main() {}
"#,
            Some("is not implemented for `Failure`"),
        ),
        (
            "different_error_sources",
            r#"
trait IFlag {}
impl IFlag for bool {}
systasis::systasis_container! {
    register_value!("42".parse::<u8>()?: u8 as Copy);
    register_value!("true".parse::<bool>()?: bool as IFlag);
}
fn main() {
    let container = SystasisContainer::build().unwrap();
    assert_eq!(container.resolve_copy(), 42);
    assert!(container.resolve_i_flag());
}
"#,
            None,
        ),
    ];
    for (name, source, failure) in cases {
        let input = target.join(format!("{name}.rs"));
        fs::write(&input, source).unwrap();
        let output = artifacts
            .rustc()
            .args(["--edition=2024", "--extern", &external, "--out-dir"])
            .arg(&target)
            .arg(&input)
            .output()
            .unwrap();
        let diagnostic = String::from_utf8_lossy(&output.stderr);
        fs::write(target.join(format!("{name}.stderr")), diagnostic.as_bytes()).unwrap();
        if let Some(reason) = failure {
            assert!(
                !output.status.success() && diagnostic.contains(reason),
                "{name}: {diagnostic}"
            );
        } else {
            assert!(output.status.success(), "{name}: {diagnostic}");
            assert!(Command::new(target.join(name)).status().unwrap().success());
        }
    }
}
