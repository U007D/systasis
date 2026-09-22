//! Parent constructor borrows exclude child ownership without restricting siblings.
#![forbid(unsafe_code)]

#[cfg(all(test, not(miri)))]
mod support;

use systasis::app_container::Error;

mod child {
    pub trait IValue {}
    impl IValue for String {}
    pub trait ISize {}
    impl ISize for usize {}
    #[systasis::container]
    pub fn run(value: String, call: impl FnOnce(&SystasisContainer)) {
        let Ok(container) = systasis::systasis_container! {
            register_value!(value: String as IValue);
            register_type_with!(usize as ISize, try || -> Result<usize, systasis::app_container::Error> {
                Ok(try_resolve!(IValue)?.len())
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
    let artifacts = support::Artifacts::build(&mut build);
    for (native, (name, access, rejection)) in [
        (
            "take",
            "container.primary().try_resolve_i_value()",
            Some("\"code\":\"E0599\""),
        ),
        (
            "indirect_take",
            "container.primary().try_resolve_i_size()",
            Some("\"code\":\"E0599\""),
        ),
        (
            "shared",
            "container.primary().try_resolve_i_value_ref()",
            None,
        ),
        (
            "mutable",
            "container.primary().try_resolve_i_value_ref_mut()",
            None,
        ),
        (
            "sibling_take",
            "container.replica().try_resolve_i_value()",
            None,
        ),
        (
            "context_cannot_restore_take",
            "{ let context = systasis::scoped::BorrowContext::borrow_context(container.primary()); context.descriptor().try_resolve_i_value() }",
            Some("\"code\":\"E0599\""),
        ),
        (
            "context_shared",
            "{ let context = systasis::scoped::BorrowContext::borrow_context(container.primary()); context.descriptor().try_resolve_i_value_ref() }",
            None,
        ),
        (
            "context_cannot_clear_mask",
            "systasis::scoped::BorrowContext::<'_, child::SystasisContainer, systasis::scoped::mask::Empty>::borrow_context(container.primary())",
            Some("\"code\":\"E0277\""),
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
        (
            "context_guard_cannot_extend_backing_lifetime",
            "{ let context = systasis::scoped::BorrowContext::borrow_context(container.primary()); let guard: systasis::Ref<'static, String> = context.descriptor().try_resolve_i_value_ref().unwrap(); guard }",
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
            register_type_with!(usize as ISize, try || -> Result<usize, systasis::app_container::Error> {{ Ok(try_resolve!(IValue)?.len()) }});
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
        register_type_with!(usize as ILength, try || -> Result<usize, systasis::app_container::Error> {{ Ok(try_resolve_ref_from!(IValue, primary)?.len()) }});
    }}.build();
    let _ = {access};
}}
fn main() {{}}
"#
        );
        let source = if native {
            source.replace(
                "Ok(try_resolve_ref_from!(IValue, primary)?.len())",
                "{ let value = try_resolve_ref_from!(IValue, primary)?; assert!(value.is_empty()); Ok(value.len()) }",
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
