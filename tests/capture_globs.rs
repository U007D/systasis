//! Closure-local globs are preserved; ambiguous outer captures are rejected.
#![forbid(unsafe_code)]
mod imported {
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
    let output = build.output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
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
    let output = Command::new("rustc")
        .args(["--edition=2024", "--emit=metadata", "--error-format=short"])
        .arg(path)
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
    fs::write(target.join("ambiguous.stderr"), &output.stderr).unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success() && stderr.contains("constructor capture analysis cannot determine whether this glob import shadows the referenced outer binding") && stderr.contains("ambiguous.rs:8:"), "{stderr}");
}
