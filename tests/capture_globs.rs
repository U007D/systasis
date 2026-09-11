//! Closure-local globs are preserved; ambiguous outer captures are rejected.
#![forbid(unsafe_code)]

#[cfg(all(test, not(miri)))]
mod support;
mod imported {
    #[allow(non_upper_case_globals)]
    pub const __systasis_slot_1: usize = 77;
    pub const NUMBER: usize = 2;
    pub fn answer() -> usize {
        3
    }
}
trait IValue {}
impl IValue for usize {}

mod independent {
    use super::*;
    #[systasis::container]
    #[test]
    fn noncapturing_glob_keeps_function_and_constant_resolution() {
        let Ok(container) = systasis::systasis_container! {
            register_type_with!(usize as IValue, || { use imported::*; NUMBER + answer() });
        }
        .build();
        assert_eq!(container.resolve_i_value(), 5);
    }
}

mod siblings {
    use super::*;
    #[systasis::container]
    #[test]
    fn sibling_glob_does_not_disable_owned_captures_elsewhere() {
        let text: String = String::from("seven");
        let Ok(container) = systasis::systasis_container! {
            register_type_with!(usize as IValue, || {
                let imported = { use imported::*; NUMBER + answer() };
                let local = text.len();
                imported + local
            });
        }
        .build();
        assert_eq!(container.resolve_i_value(), 10);
    }
}

mod guarded {
    use super::imported;
    use systasis::app_container::Error;
    struct View<'a>(systasis::Ref<'a, String>);
    trait IView {}
    impl IView for View<'_> {}
    trait IValue {}
    impl IValue for String {}
    #[systasis::container]
    #[test]
    fn fallible_glob_constructor_retains_returned_guard() -> Result<(), Error> {
        let Ok(container) = systasis::systasis_container! {
            register_type_with!(View<'_> as IView, try || -> Result<View<'_>, Error> {
                use imported::*;
                let _ = answer();
                let _authored_slot = __systasis_slot_1;
                Ok(View(try_resolve_ref!(IValue)?))
            });
            register_value!(String::from("value"): String as IValue);
        }
        .build();
        let view = container.try_resolve_i_view()?;
        assert_eq!(&*view.0, "value");
        assert!(matches!(
            container.try_resolve_i_value_ref_mut(),
            Err(Error::ValueAccessContention)
        ));
        drop(view);
        container.try_resolve_i_value_ref_mut()?.push('!');
        assert_eq!(&*container.try_resolve_i_view()?.0, "value!");
        Ok(())
    }
}

#[test]
fn rust_control_glob_shadows_outer_callable() {
    #[allow(unused_variables)]
    let answer: usize = 7;
    let closure = || {
        use imported::*;
        answer() + NUMBER
    };
    assert_eq!(closure(), 5);
}

mod child_hygiene {
    use systasis::app_container::Error;
    trait IDatabase {}
    impl IDatabase for String {}

    mod imported {
        #[allow(non_upper_case_globals)]
        pub const __systasis_children: usize = 100;
        pub mod __systasis_injected {
            pub fn construct_0() -> usize {
                900
            }
        }
    }

    mod outer {
        use super::Error;
        trait ILength {}
        impl ILength for usize {}
        trait IIndirect {}
        impl IIndirect for usize {}

        #[systasis::container]
        pub fn run(primary: &super::AppContainer) -> Result<(), Error> {
            let Ok(container) = systasis::systasis_container! {
                register_container!(primary: &super::AppContainer);
                register_type_with!(usize as ILength, try || -> Result<usize, Error> {
                    use super::imported::*;
                    let _authored = __systasis_children;
                    Ok(try_resolve_ref_from!(IDatabase, primary)?.len())
                });
                register_type_with!(usize as IIndirect, try || -> Result<usize, Error> {
                    use super::imported::*;
                    let _authored = __systasis_children;
                    let _authored_module = __systasis_injected::construct_0();
                    try_resolve!(ILength)
                });
            }
            .build();
            assert_eq!(container.try_resolve_i_length()?, 5);
            assert_eq!(container.try_resolve_i_indirect()?, 5);
            Ok(())
        }
    }

    #[systasis::container]
    #[test]
    fn imported_names_do_not_replace_generated_child_storage() -> Result<(), Error> {
        let Ok(container) = systasis::systasis_container! {
            register_value!(String::from("value"): String as IDatabase);
        }
        .build();
        outer::run(container)
    }
}

mod caller_binding_hygiene {
    trait IValue {}
    impl IValue for usize {}

    #[systasis::container]
    #[test]
    fn child_storage_does_not_shadow_an_authored_initializer_input() {
        let __systasis_children: usize = 41;
        let Ok(container) = systasis::systasis_container! {
            register_value!(__systasis_children: usize as IValue);
        }
        .build();
        assert_eq!(container.resolve_i_value(), 41);
        assert_eq!(__systasis_children, 41);
    }
}

mod unused_context_generics {
    use core::marker::PhantomData;
    use systasis::app_container::Error;
    struct Number(u32);
    trait INumber {}
    impl INumber for Number {}
    struct View<'a>(systasis::Ref<'a, Number>);
    trait IView {}
    impl IView for View<'_> {}
    trait IText {}
    impl IText for &str {}
    #[systasis::container]
    fn run<'short, 'long, T>(
        _: PhantomData<fn() -> (&'short (), T)>,
        text: &'long str,
    ) -> Result<&'long str, Error> {
        let Ok(container) = systasis::systasis_container! {
            register_value!(Number(17): Number as INumber);
            register_type_with!(View<'_> as IView, try || -> Result<View<'_>, Error> {
                Ok(View(try_resolve_ref!(INumber)?))
            });
            register_type_with!(&'long str as IText, || text);
        }
        .build();
        let view = container.try_resolve_i_view()?;
        assert_eq!(view.0.0, 17);
        Ok(container.resolve_i_text())
    }
    #[test]
    fn unrelated_generic_markers_do_not_change_guard_or_capture_outputs() {
        let text = String::from("long");
        let returned = run::<&str>(PhantomData, &text).unwrap();
        assert_eq!(returned, "long");
    }
}

#[test]
#[cfg(not(miri))]
fn potentially_shadowed_capture_reports_the_ambiguity() {
    use std::{fs, path::Path, process::Command};
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let target = root
        .join("target/capture-globs")
        .join(if cfg!(feature = "std") {
            "std"
        } else {
            "no-std"
        });
    let mut build = Command::new(env!("CARGO"));
    build
        .current_dir(root)
        .args(["build", "--lib", "--offline", "--locked", "--target-dir"])
        .arg(&target);
    if !cfg!(feature = "std") {
        build.arg("--no-default-features");
    }
    let artifacts = support::Artifacts::build(&mut build);
    let source = r#"
mod imported { pub fn answer() -> usize { 3 } }
trait IValue {} impl IValue for usize {}
#[systasis::container]
fn main() {
    let answer: usize = 7;
    let Ok(_) = systasis::systasis_container! {
        register_type_with!(usize as IValue, || { use imported::*; answer() });
    }.build();
}
"#;
    let path = target.join("ambiguous.rs");
    fs::write(&path, source).unwrap();
    let output = artifacts
        .rustc()
        .args(["--edition=2024", "--emit=metadata", "--error-format=short"])
        .arg(path)
        .arg("--out-dir")
        .arg(&target)
        .output()
        .unwrap();
    fs::write(target.join("ambiguous.stderr"), &output.stderr).unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success() && stderr.contains("constructor capture analysis cannot determine whether this glob import shadows the referenced outer binding") && stderr.contains("ambiguous.rs:8:"), "{stderr}");
}
