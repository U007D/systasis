//! Compiler-selected locals do not consume expansion depth per condition.
#![cfg(not(miri))]
#![forbid(unsafe_code)]

#[cfg(all(test, not(miri)))]
mod support;

use std::{fmt::Write as _, fs, path::Path, process::Command};

#[test]
fn many_conditions_preserve_ancestry_hygiene_and_module_scope() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let target = root
        .join("target/configured-scaling")
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

    let mut source = String::from(
        r#"
#![forbid(unsafe_code)]
#![deny(warnings)]
#![recursion_limit = "64"]
trait IValue {}
impl IValue for u32 {}
#[allow(non_camel_case_types, dead_code)]
struct __SystasisConfiguration_main {}
fn named(container: &AppContainer) -> u32 { container.resolve_i_value() }
#[systasis::container]
#[cfg(any())]
fn disabled_function_with_condition() {
    #[cfg(invalid_predicate(foo))]
    let invalid: u8 = missing;
    let Ok(container) = systasis::systasis_container! { register_value!(3u32: u32 as IValue); }.build();
    let _ = container;
}
#[systasis::container]
#[cfg(any())]
fn disabled_function_without_condition() {
    let Ok(container) = systasis::systasis_container! { register_value!(3u32: u32 as IValue); }.build();
    let _ = container;
}
#[systasis::container(require(Send, Sync))]
#[cfg_attr(all(), cfg_attr(all(), cfg(any())))]
fn disabled_function_with_nested_attribute() {
    #[cfg(invalid_predicate(foo))]
    let invalid: u8 = missing;
    let Ok(container) = systasis::systasis_container! { register_value!(3u32: u32 as IValue); }.build();
    let _ = container;
}
#[inline]
#[cfg(any())]
fn ordinary_disabled_function() {
    #[cfg(invalid_predicate(foo))]
    let invalid: u8 = missing;
}
fn ordinary() {
    #[cfg(any())]
    #[cfg()]
    let invalid_syntax = missing;
    #[cfg_attr(any(), cfg(invalid_predicate(foo)))]
    let _value = 1;
    #[cfg(any())]
    let unused: () = { #[cfg(invalid_predicate(foo))] let bad: u8 = missing; };
    #[cfg(any())]
    #[cfg(invalid_predicate(foo))]
    let unused: u8 = missing;
}
#[systasis::container]
fn main() {
    ordinary(); first::run(); second::run();
    #[cfg(any())]
    #[cfg()]
    let invalid_syntax = missing;
    #[allow(non_snake_case)]
    let __SystasisConfiguration_main: u32 = 3;
    #[cfg_attr(any(), cfg(invalid_predicate(foo)))]
    let _value = 1;
    #[cfg(any())]
    let unused: () = { #[cfg(invalid_predicate(foo))] let bad: u8 = missing; };
    #[cfg(any())]
    #[cfg(invalid_predicate(foo))]
    let unused: u8 = missing;
"#,
    );
    for index in 0..256 {
        writeln!(source, "#[cfg_attr(all(), cfg_attr(all(), cfg(all())))] #[allow(unused_variables)] let value_{index}: u32 = {index};").unwrap();
    }
    source.push_str(
        r#"
    let Ok(container) = systasis::systasis_container! {
        register_type_with!(u32 as IValue, move || __SystasisConfiguration_main + value_255);
    }.build();
    assert_eq!(named(container), 258);
}
"#,
    );
    for module in ["first", "second"] {
        writeln!(
            source,
            r#"
mod {module} {{
    trait IValue {{}} impl IValue for u32 {{}}
    fn named(container: &AppContainer) -> u32 {{ container.resolve_i_value() }}
    #[systasis::container]
    #[cfg(all())]
    #[inline]
    pub fn run() {{
        #[cfg(all())] let value: u32 = 3;
        let Ok(container) = systasis::systasis_container! {{
            register_type_with!(u32 as IValue, move || value);
        }}.build();
        assert_eq!(named(container), 3);
    }}
}}
"#
        )
        .unwrap();
    }
    let invalid = r#"
trait IValue {} impl IValue for u32 {}
#[systasis::container]
fn main() {
    #[cfg_attr(all(), cfg(invalid_predicate(foo)))]
    let value: u32 = 3;
    let Ok(container) = systasis::systasis_container! {
        register_type_with!(u32 as IValue, move || value);
    }.build();
    assert_eq!(container.resolve_i_value(), 3);
}
"#;
    let control =
        "fn main() { #[cfg_attr(all(), cfg(invalid_predicate(foo)))] let _value: u32 = 3; }";
    for (name, source, success) in [
        ("selected", source.as_str(), true),
        ("invalid", invalid, false),
        ("control", control, false),
    ] {
        let file = target.join(format!("{name}.rs"));
        fs::write(&file, source).expect("write configuration fixture");
        let output = artifacts
            .rustc()
            .args(["--edition=2024", "--crate-name", name])
            .arg(&file)
            .arg("--out-dir")
            .arg(&target)
            .output()
            .expect("compile configuration fixture");
        let diagnostics = String::from_utf8_lossy(&output.stderr);
        fs::write(target.join(format!("{name}.log")), diagnostics.as_bytes())
            .expect("retain diagnostics");
        assert_eq!(output.status.success(), success, "{name}: {diagnostics}");
        if success {
            assert!(
                Command::new(target.join(name))
                    .status()
                    .expect("execute configuration fixture")
                    .success()
            );
        } else {
            assert!(
                diagnostics.lines().any(|line| {
                    // rustc versions differ in whether this unknown
                    // predicate is reported directly or as malformed cfg.
                    line.starts_with("error[E0537]: invalid predicate `invalid_predicate`")
                        || line.starts_with("error[E0539]: malformed `cfg` attribute input")
                }),
                "{diagnostics}"
            );
            assert!(
                diagnostics.contains(&format!("--> {}:", file.display())),
                "{diagnostics}"
            );
        }
    }
}
