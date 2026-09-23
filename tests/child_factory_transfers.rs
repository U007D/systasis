//! Consuming child calls propagate through local factory chains without blocking siblings.
#![forbid(unsafe_code)]

#[cfg(all(test, not(miri)))]
mod support;
use systasis::container::Error;

mod leaf {
    pub trait IValue {}
    pub struct Value(pub String);
    impl IValue for Value {}
    #[systasis::container]
    pub fn run(value: String, call: impl FnOnce(&SystasisContainer)) {
        let Ok(container) = systasis::systasis_container! {
            register_value!(Value(value): Value as IValue);
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
    pub fn build<'a>(
        primary: &'a leaf::SystasisContainer,
        replica: &'a leaf::SystasisContainer,
    ) -> SystasisContainer<'a> {
        let Ok(container) = systasis::systasis_container! {
            register_container!(primary: &'a leaf::SystasisContainer);
            register_container!(replica: &'a leaf::SystasisContainer);
            register_type_with!(usize as IFirst, try || -> Result<usize, Error> {
                Ok(try_resolve_from!(IValue, primary)?.0.len())
            });
            register_type_with!(usize as ISecond, try || -> Result<usize, Error> {
                try_resolve!(IFirst)
            });
            register_type_with!(usize as ISibling, try || -> Result<usize, Error> {
                Ok(try_resolve_from!(IValue, replica)?.0.len())
            });
        }
        .build();
        container
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
                try_resolve_from!(ISecond, branch)
            });
        }
        .build::<Error>()?;
        assert_eq!(container.try_resolve_i_observed()?, 7);
        assert_eq!(container.branch().try_resolve_i_sibling()?, 7);
        assert!(matches!(
            container.branch().try_resolve_i_sibling(),
            Err(Error::ValueAlreadyConsumed)
        ));
        assert!(matches!(
            container.try_resolve_i_observed(),
            Err(Error::ValueAlreadyConsumed)
        ));
        assert!(matches!(
            container.branch().try_resolve_i_first(),
            Err(Error::ValueAlreadyConsumed)
        ));
        assert!(matches!(
            container.branch().try_resolve_i_second(),
            Err(Error::ValueAlreadyConsumed)
        ));
        Ok(())
    }
}

#[test]
fn nested_factory_chains_transfer_once_and_leave_siblings_independent() {
    leaf::run(String::from("primary"), |primary| {
        leaf::run(String::from("replica"), |replica| {
            let branch = middle::build(primary, replica);
            outer::run(&branch).unwrap();
        });
    });
}

#[test]
#[cfg(not(miri))]
fn source_and_native_constructor_chains_can_transfer_child_values() {
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
    for (native, interface) in ["IFirst", "ISecond"]
        .into_iter()
        .flat_map(|interface| [false, true].map(|native| (native, interface)))
    {
        let source = include_str!("child_factory_transfers.rs").replace(
            concat!("try_resolve_from!", "(ISecond, branch)"),
            &format!("try_resolve_from!({interface}, branch)"),
        );
        let source = if native {
            source
                .replace(
                    &format!("try_resolve_from!({interface}, branch)"),
                    &format!("{{ let length = try_resolve_from!({interface}, branch)?; assert!(length > 0); Ok(length) }}"),
                )
                .replace(
                    "try_resolve!(IFirst)",
                    "{ let length = try_resolve!(IFirst)?; assert!(length > 0); Ok(length) }",
                )
        } else {
            source
        };
        let source = format!(
            "{source}\nfn main() {{ leaf::run(String::from(\"primary\"), |primary| leaf::run(String::from(\"replica\"), |replica| {{ let branch = middle::build(primary, replica); outer::run(&branch).unwrap(); }})); }}\n"
        );
        let name = format!("{interface}_native_{native}").to_lowercase();
        let path = target.join(format!("{name}.rs"));
        fs::write(&path, source).unwrap();
        let output = artifacts
            .rustc()
            .args(["--edition=2024", "--error-format=json"])
            .arg(&path)
            .arg("--out-dir")
            .arg(&target)
            .output()
            .unwrap();
        let diagnostics = String::from_utf8_lossy(&output.stderr);
        fs::write(target.join(format!("{name}.jsonl")), &output.stderr).unwrap();
        assert!(output.status.success(), "{name}: {diagnostics}");
        let output = Command::new(target.join(&name)).output().unwrap();
        assert!(
            output.status.success(),
            "{name}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
