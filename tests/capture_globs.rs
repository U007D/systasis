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

mod cloned_output {
    use super::imported;
    use systasis::container::Error;
    struct View(String);
    trait IView {}
    impl IView for View {}
    trait IValue {}
    impl IValue for String {}
    #[systasis::container]
    #[test]
    fn fallible_glob_constructor_returns_an_independent_clone() -> Result<(), Error> {
        let Ok(container) = systasis::systasis_container! {
            register_type_with!(View as IView, try || -> Result<View, Error> {
                use imported::*;
                let _ = answer();
                let _authored_slot = __systasis_slot_1;
                Ok(View(resolve_clone!(IValue)))
            });
            register_value!(String::from("value"): String as IValue);
        }
        .build();
        let mut view = container.try_resolve_i_view()?;
        assert_eq!(view.0, "value");
        view.0.push('!');
        assert_eq!(view.0, "value!");
        assert_eq!(container.try_resolve_i_view()?.0, "value");
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
    use systasis::container::Error;
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
        pub fn run(primary: &super::SystasisContainer) -> Result<(), Error> {
            let Ok(container) = systasis::systasis_container! {
                register_container!(primary: &super::SystasisContainer);
                register_type_with!(usize as ILength, try || -> Result<usize, Error> {
                    use super::imported::*;
                    let _authored = __systasis_children;
                    Ok(resolve_clone_from!(IDatabase, primary).len())
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
        outer::run(&container)
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
    use systasis::container::Error;
    struct Number(u32);
    trait INumber {}
    impl INumber for Number {}
    struct View(Number);
    trait IView {}
    impl IView for View {}
    trait IText {}
    impl IText for &str {}
    #[systasis::container]
    fn run<'short, 'long, T>(
        _: PhantomData<fn() -> (&'short (), T)>,
        text: &'long str,
    ) -> Result<&'long str, Error> {
        let Ok(container) = systasis::systasis_container! {
            register_value!(Number(17): Number as INumber);
            register_type_with!(View as IView, try || -> Result<View, Error> {
                Ok(View(try_resolve!(INumber)?))
            });
            register_type_with!(&'long str as IText, || text);
        }
        .build();
        let view = container.try_resolve_i_view()?;
        assert_eq!(view.0.0, 17);
        Ok(container.resolve_i_text())
    }
    #[test]
    fn unrelated_generic_markers_do_not_change_owned_or_capture_outputs() {
        let text = String::from("long");
        let returned = run::<&str>(PhantomData, &text).unwrap();
        assert_eq!(returned, "long");
    }
}

#[test]
#[cfg(not(miri))]
fn potentially_shadowed_capture_uses_rust_name_resolution() {
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
#![forbid(unsafe_code)]
mod imported { pub fn answer() -> usize { 3 } }
trait IValue {} impl IValue for usize {}
#[systasis::container]
fn main() {
    let answer: String = String::from("caller");
    let Ok(container) = systasis::systasis_container! {
        register_type_with!(usize as IValue, || { use imported::*; answer() });
    }.build();
    assert_eq!(container.resolve_i_value(), 3);
    assert_eq!(container.resolve_i_value(), 3);
    assert_eq!(answer, "caller");
}
"#;
    let path = target.join("ambiguous.rs");
    fs::write(&path, source).unwrap();
    let binary = target.join("ambiguous");
    let output = artifacts
        .rustc()
        .args(["--edition=2024", "-Dwarnings", "--error-format=short"])
        .arg(path)
        .arg("-o")
        .arg(&binary)
        .output()
        .unwrap();
    fs::write(target.join("ambiguous.stderr"), &output.stderr).unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stderr}");
    assert!(Command::new(binary).status().unwrap().success());
}
