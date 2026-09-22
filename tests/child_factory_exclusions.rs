//! Consuming child calls propagate through local factory chains without blocking siblings.
#![forbid(unsafe_code)]

#[cfg(all(test, not(miri)))]
mod support;
use systasis::app_container::Error;

mod leaf {
    pub trait IValue {}
    impl IValue for String {}
    #[systasis::container]
    pub fn run(value: String, call: impl FnOnce(&SystasisContainer)) {
        let Ok(container) = systasis::systasis_container! {
            register_value!(value: String as IValue);
        }
        .build();
        call(&container);
    }
}

mod middle {
    use super::*;
    trait IFirst {}
    trait ISecond {}
    trait ISibling {}
    impl IFirst for usize {}
    impl ISecond for usize {}
    impl ISibling for usize {}
    #[systasis::container]
    pub fn run<'a>(
        primary: &'a leaf::SystasisContainer,
        replica: &'a leaf::SystasisContainer,
        call: impl FnOnce(&SystasisContainer<'a>),
    ) {
        let Ok(container) = systasis::systasis_container! {
            register_container!(primary: &'a leaf::SystasisContainer);
            register_container!(replica: &'a leaf::SystasisContainer);
            register_type_with!(usize as IFirst, try || -> Result<usize, Error> {
                Ok(try_resolve_from!(IValue, primary)?.len())
            });
            register_type_with!(usize as ISecond, try || -> Result<usize, Error> {
                try_resolve!(IFirst)
            });
            register_type_with!(usize as ISibling, try || -> Result<usize, Error> {
                Ok(try_resolve_from!(IValue, replica)?.len())
            });
        }
        .build();
        call(&container);
    }
}

mod outer {
    use super::*;
    trait IObserved {}
    impl IObserved for usize {}
    #[systasis::container]
    pub fn run<'a>(branch: &middle::SystasisContainer<'a>) -> Result<(), Error> {
        let container = systasis::systasis_container! {
            register_container!(branch: &middle::SystasisContainer<'a>);
            register_type_with!(usize as IObserved, try || -> Result<usize, Error> {
                Ok(try_resolve_ref_from!(IValue, branch::primary)?.len())
            });
        }
        .build::<Error>()?;
        assert_eq!(container.try_resolve_i_observed()?, 7);
        assert_eq!(container.branch().try_resolve_i_sibling()?, 7);
        assert!(matches!(
            container.branch().try_resolve_i_sibling(),
            Err(Error::ValueAlreadyConsumed)
        ));
        assert_eq!(container.try_resolve_i_observed()?, 7);
        // EXCLUDED_FACTORY
        Ok(())
    }
}

#[test]
fn unrelated_sibling_factory_remains_consumable() {
    leaf::run(String::from("primary"), |primary| {
        leaf::run(String::from("replica"), |replica| {
            middle::run(primary, replica, |branch| outer::run(branch).unwrap());
        });
    });
}

#[test]
#[cfg(not(miri))]
fn direct_and_transitive_child_consuming_factories_are_excluded() {
    use std::{fs, path::PathBuf, process::Command};
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let backend = if cfg!(feature = "std") {
        "std"
    } else {
        "no-std"
    };
    let target = root.join("target/child-factory-contracts").join(backend);
    let mut build = Command::new(env!("CARGO"));
    build
        .current_dir(&root)
        .args(["build", "--lib", "--offline", "--locked", "--target-dir"])
        .arg(&target);
    if !cfg!(feature = "std") {
        build.arg("--no-default-features");
    }
    let artifacts = support::Artifacts::build(&mut build);
    for (native, method) in ["try_resolve_i_first", "try_resolve_i_second"]
        .into_iter()
        .flat_map(|method| [false, true].map(|native| (native, method)))
    {
        let source = include_str!("child_factory_exclusions.rs").replace(
            concat!("// EXCLUDED", "_FACTORY"),
            &format!("let _ = container.branch().{method}();"),
        );
        let source = if native {
            source
                .replace(
                    "Ok(try_resolve_ref_from!(IValue, branch::primary)?.len())",
                    "{ let value = try_resolve_ref_from!(IValue, branch::primary)?; assert!(!value.is_empty()); Ok(value.len()) }",
                )
                .replace(
                    "try_resolve!(IFirst)",
                    "{ let length = try_resolve!(IFirst)?; assert!(length > 0); Ok(length) }",
                )
        } else {
            source
        };
        let name = format!("{method}_native_{native}");
        let path = target.join(format!("{name}.rs"));
        fs::write(&path, source).unwrap();
        let output = artifacts
            .rustc()
            .args([
                "--edition=2024",
                "--crate-type=lib",
                "--emit=metadata",
                "--error-format=json",
            ])
            .arg(&path)
            .arg("--out-dir")
            .arg(&target)
            .output()
            .unwrap();
        let diagnostics = String::from_utf8_lossy(&output.stderr);
        fs::write(target.join(format!("{name}.jsonl")), &output.stderr).unwrap();
        assert!(!output.status.success(), "{name} unexpectedly compiled");
        assert!(
            diagnostics.contains("\"code\":\"E0599\""),
            "{name}: {diagnostics}"
        );
    }
}
