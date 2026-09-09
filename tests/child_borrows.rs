//! Parent constructor borrows exclude child ownership without restricting siblings.
#![forbid(unsafe_code)]

use systasis::app_container::Error;

mod child {
    pub trait IValue {}
    impl IValue for String {}
    pub trait ISize {}
    impl ISize for usize {}
    #[systasis::container]
    pub fn run(value: String, call: impl FnOnce(&AppContainer)) {
        let Ok(container) = systasis::systasis_container! {
            register_value!(value: String as IValue);
            register_type_with!(usize as ISize, try || -> Result<usize, systasis::app_container::Error> {
                Ok(try_resolve!(IValue)?.len())
            });
        }.build();
        call(container);
    }
}

mod parent {
    use super::*;
    trait ILength {}
    impl ILength for usize {}
    #[systasis::container]
    pub fn run(primary: &child::AppContainer, replica: &child::AppContainer) -> Result<(), Error> {
        let container = systasis::systasis_container! {
            register_container!(primary: &child::AppContainer);
            register_container!(replica: &child::AppContainer);
            register_type_with!(usize as ILength, try || -> Result<usize, Error> {
                Ok(try_resolve_ref_from!(IValue, primary)?.len())
            });
        }
        .build::<Error>()?;
        assert_eq!(container.try_resolve_i_length()?, 7);
        assert_eq!(&*container.primary().try_resolve_i_value_ref()?, "primary");
        container.primary().try_resolve_i_value_ref_mut()?.push('!');
        assert_eq!(container.try_resolve_i_length()?, 8);
        assert_eq!(container.replica().try_resolve_i_value()?, "replica");
        Ok(())
    }
}

#[test]
fn child_borrow_retains_mutable_access_and_does_not_restrict_sibling() {
    child::run(String::from("primary"), |primary| {
        child::run(String::from("replica"), |replica| {
            parent::run(primary, replica).unwrap()
        });
    });
}

#[test]
#[cfg(not(miri))]
fn borrowed_child_cannot_be_consumed_directly_or_through_its_factory() {
    use std::{fs, path::PathBuf, process::Command};
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let backend = if cfg!(feature = "std") {
        "std"
    } else {
        "no-std"
    };
    let target = root.join("target/child-borrow-contracts").join(backend);
    let mut build = Command::new(env!("CARGO"));
    build
        .current_dir(&root)
        .args(["build", "--lib", "--offline", "--locked", "--target-dir"])
        .arg(&target);
    if !cfg!(feature = "std") {
        build.arg("--no-default-features");
    }
    let output = build.output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    for (name, access, rejected) in [
        ("take", "container.primary().try_resolve_i_value()", true),
        (
            "indirect_take",
            "container.primary().try_resolve_i_size()",
            true,
        ),
        (
            "shared",
            "container.primary().try_resolve_i_value_ref()",
            false,
        ),
        (
            "mutable",
            "container.primary().try_resolve_i_value_ref_mut()",
            false,
        ),
        (
            "sibling_take",
            "container.replica().try_resolve_i_value()",
            false,
        ),
    ] {
        let source = format!(
            r#"
mod child {{
    pub trait IValue {{}} impl IValue for String {{}}
    pub trait ISize {{}} impl ISize for usize {{}}
    #[systasis::container]
    fn build() {{
        let Ok(container) = systasis::systasis_container! {{
            register_value!(String::new(): String as IValue);
            register_type_with!(usize as ISize, try || -> Result<usize, systasis::app_container::Error> {{ Ok(try_resolve!(IValue)?.len()) }});
        }}.build();
    }}
}}
use child::IValue;
trait ILength {{}} impl ILength for usize {{}}
#[systasis::container]
fn parent(primary: &child::AppContainer, replica: &child::AppContainer) {{
    let Ok(container) = systasis::systasis_container! {{
        register_container!(primary: &child::AppContainer);
        register_container!(replica: &child::AppContainer);
        register_type_with!(usize as ILength, try || -> Result<usize, systasis::app_container::Error> {{ Ok(try_resolve_ref_from!(IValue, primary)?.len()) }});
    }}.build();
    let _ = {access};
}}
fn main() {{}}
"#
        );
        let path = target.join(format!("{name}.rs"));
        fs::write(&path, source).unwrap();
        let output = Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata", "--error-format=json"])
            .arg(&path)
            .arg("--out-dir")
            .arg(&target)
            .arg("--extern")
            .arg(format!(
                "systasis={}",
                target.join("debug/libsystasis.rlib").display()
            ))
            .arg("-L")
            .arg(format!(
                "dependency={}",
                target.join("debug/deps").display()
            ))
            .output()
            .unwrap();
        let diagnostics = String::from_utf8_lossy(&output.stderr);
        if rejected {
            assert!(!output.status.success(), "{name} unexpectedly compiled");
            assert!(
                diagnostics.contains("\"code\":\"E0599\""),
                "{name}: {diagnostics}"
            );
        } else {
            assert!(output.status.success(), "{name}: {diagnostics}");
        }
    }
}
