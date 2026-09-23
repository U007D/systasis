//! Parent constructors clone child registrations without changing either sibling.
#![forbid(unsafe_code)]

#[cfg(all(test, not(miri)))]
mod support;

use systasis::container::Error;

mod child {
    pub trait IValue {}
    impl IValue for String {}
    pub trait ISize {}
    impl ISize for usize {}
    #[systasis::container]
    pub fn run(value: String, call: impl FnOnce(&SystasisContainer)) {
        let Ok(container) = systasis::systasis_container! {
            register_value!(value: String as IValue);
            register_type_with!(usize as ISize, try || -> Result<usize, systasis::container::Error> {
                Ok(resolve_clone!(IValue).len())
            });
        }.build();
        call(&container);
    }
}

mod parent {
    use super::*;
    trait ILength {}
    impl ILength for usize {}
    #[systasis::container]
    pub fn run(
        primary: &child::SystasisContainer,
        replica: &child::SystasisContainer,
    ) -> Result<(), Error> {
        let container = systasis::systasis_container! {
            register_container!(primary: &child::SystasisContainer);
            register_container!(replica: &child::SystasisContainer);
            register_type_with!(usize as ILength, try || -> Result<usize, Error> {
                Ok(resolve_clone_from!(IValue, primary).len())
            });
        }
        .build::<Error>()?;
        assert_eq!(container.try_resolve_i_length()?, 7);
        let mut cloned = container.primary().resolve_clone_i_value();
        assert_eq!(cloned, "primary");
        cloned.push('!');
        assert_eq!(cloned, "primary!");
        assert_eq!(container.try_resolve_i_length()?, 7);
        assert_eq!(container.primary().try_resolve_i_size()?, 7);
        assert_eq!(container.replica().resolve_clone_i_value(), "replica");
        Ok(())
    }
}

#[test]
fn cloned_child_values_are_independent_and_do_not_restrict_siblings() {
    child::run(String::from("primary"), |primary| {
        child::run(String::from("replica"), |replica| {
            parent::run(primary, replica).unwrap()
        });
    });
}

#[test]
#[cfg(not(miri))]
fn child_context_preserves_clone_access_visibility_and_backing_lifetime() {
    use std::{fs, path::PathBuf, process::Command};
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let backend = if cfg!(feature = "std") {
        "std"
    } else {
        "no-std"
    };
    let target = root.join("target/child-clone-contracts").join(backend);
    let mut build = Command::new(env!("CARGO"));
    build
        .current_dir(&root)
        .args(["build", "--lib", "--offline", "--locked", "--target-dir"])
        .arg(&target);
    if !cfg!(feature = "std") {
        build.arg("--no-default-features");
    }
    let artifacts = support::Artifacts::build(&mut build);
    for (native, (name, access, rejection)) in [
        (
            "clone",
            "container.primary().resolve_clone_i_value()",
            None,
        ),
        (
            "factory",
            "container.primary().try_resolve_i_size()",
            None,
        ),
        (
            "sibling_clone",
            "container.replica().resolve_clone_i_value()",
            None,
        ),
        (
            "context_clone",
            "{ let context = systasis::scoped::BorrowContext::borrow_context(container.primary()); context.descriptor().resolve_clone_i_value() }",
            None,
        ),
        (
            "context_with_empty_mask",
            "systasis::scoped::BorrowContext::<'_, child::SystasisContainer, systasis::scoped::mask::Empty>::borrow_context(container.primary())",
            None,
        ),
        (
            "context_backing_is_private",
            "systasis::scoped::BorrowContext::borrow_context(container.primary()).backing",
            Some("\"code\":\"E0616\""),
        ),
        (
            "context_cannot_extend_backing_lifetime",
            "{ let context: systasis::scoped::BorrowedContext<'static, child::SystasisContainer, _> = systasis::scoped::BorrowContext::borrow_context(container.primary()); context }",
            Some("lifetime may not live long enough"),
        ),
    ]
    .into_iter()
    .flat_map(|case| [false, true].map(|native| (native, case)))
    {
        let name = format!("{name}_native_{native}");
        let source = format!(
            r#"
mod child {{
    pub trait IValue {{}} impl IValue for String {{}}
    pub trait ISize {{}} impl ISize for usize {{}}
    #[systasis::container]
    fn build() {{
        let Ok(container) = systasis::systasis_container! {{
            register_value!(String::new(): String as IValue);
            register_type_with!(usize as ISize, try || -> Result<usize, systasis::container::Error> {{ Ok(resolve_clone!(IValue).len()) }});
        }}.build();
    }}
}}
use child::IValue;
trait ILength {{}} impl ILength for usize {{}}
#[systasis::container]
fn parent(primary: &child::SystasisContainer, replica: &child::SystasisContainer) {{
    let Ok(container) = systasis::systasis_container! {{
        register_container!(primary: &child::SystasisContainer);
        register_container!(replica: &child::SystasisContainer);
        register_type_with!(usize as ILength, try || -> Result<usize, systasis::container::Error> {{ Ok(resolve_clone_from!(IValue, primary).len()) }});
    }}.build();
    let _ = {access};
}}
fn main() {{}}
"#
        );
        let source = if native {
            source.replace(
                "Ok(resolve_clone_from!(IValue, primary).len())",
                "{ let value = resolve_clone_from!(IValue, primary); assert!(value.is_empty()); Ok(value.len()) }",
            )
        } else {
            source
        };
        let path = target.join(format!("{name}.rs"));
        fs::write(&path, source).unwrap();
        let output = artifacts
            .rustc()
            .args(["--edition=2024", "--emit=metadata", "--error-format=json"])
            .arg(&path)
            .arg("--out-dir")
            .arg(&target)
            .output()
            .unwrap();
        let diagnostics = String::from_utf8_lossy(&output.stderr);
        if let Some(rejection) = rejection {
            assert!(!output.status.success(), "{name} unexpectedly compiled");
            assert!(diagnostics.contains(rejection), "{name}: {diagnostics}");
        } else {
            assert!(output.status.success(), "{name}: {diagnostics}");
        }
    }
}
