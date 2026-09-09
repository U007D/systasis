//! Nested descriptors retain child exclusions and independent sibling state.
#![forbid(unsafe_code)]

use systasis::app_container::Error;

mod leaf {
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

mod middle {
    use super::*;
    trait ILength {}
    impl ILength for usize {}
    #[systasis::container]
    pub fn run<'a>(
        primary: &'a leaf::AppContainer,
        replica: &'a leaf::AppContainer,
        call: impl FnOnce(&AppContainer<'a>),
    ) {
        let Ok(container) = systasis::systasis_container! {
            register_container!(primary: &'a leaf::AppContainer);
            register_container!(replica: &'a leaf::AppContainer);
            register_type_with!(usize as ILength, try || -> Result<usize, Error> {
                Ok(try_resolve_ref_from!(IValue, primary)?.len())
            });
        }
        .build();
        call(container);
    }
}

mod outer {
    use super::*;
    fn receive(scope: &branch::SubContainer<'_, '_>) -> Result<usize, Error> {
        scope.try_resolve_i_length()
    }
    #[systasis::container]
    pub fn run<'a>(branch: &middle::AppContainer<'a>) -> Result<(), Error> {
        let container = systasis::systasis_container! {
            register_container!(branch: &middle::AppContainer<'a>);
        }
        .build::<Error>()?;
        assert_eq!(receive(container.branch())?, 7);
        assert_eq!(
            &*container.branch().primary().try_resolve_i_value_ref()?,
            "primary"
        );
        container
            .branch()
            .primary()
            .try_resolve_i_value_ref_mut()?
            .push('!');
        assert_eq!(receive(container.branch())?, 8);
        assert_eq!(
            container.branch().replica().try_resolve_i_value()?,
            "replica"
        );
        // NEGATIVE_ACCESS
        Ok(())
    }
}

#[test]
fn nested_scopes_preserve_borrows_and_leave_siblings_consumable() {
    leaf::run(String::from("primary"), |primary| {
        leaf::run(String::from("replica"), |replica| {
            middle::run(primary, replica, |branch| outer::run(branch).unwrap());
        });
    });
}

#[test]
#[cfg(not(miri))]
fn nested_scopes_cannot_restore_direct_or_indirect_ownership() {
    use std::{fs, path::PathBuf, process::Command};
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let backend = if cfg!(feature = "std") {
        "std"
    } else {
        "no-std"
    };
    let target = root.join("target/nested-child-contracts").join(backend);
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
    for method in ["try_resolve_i_value", "try_resolve_i_size"] {
        let source = include_str!("nested_children.rs").replace(
            concat!("// NEGATIVE", "_ACCESS"),
            &format!("let _ = container.branch().primary().{method}();"),
        );
        let path = target.join(format!("{method}.rs"));
        fs::write(&path, source).unwrap();
        let output = Command::new("rustc")
            .args([
                "--edition=2024",
                "--crate-type=lib",
                "--emit=metadata",
                "--error-format=json",
            ])
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
        assert!(!output.status.success(), "{method} unexpectedly compiled");
        assert!(
            diagnostics.contains("\"code\":\"E0599\""),
            "{method}: {diagnostics}"
        );
    }
}
