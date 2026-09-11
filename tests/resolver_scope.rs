//! Resolver names belong to the selected registration catalog, not lexical scope.
#![forbid(unsafe_code)]

#[cfg(all(test, not(miri)))]
mod support;

trait IValue {}
impl IValue for u32 {}
trait IOutput {}
impl IOutput for u32 {}

mod unavailable {
    trait IValue {}
    impl IValue for String {}

    #[systasis::container]
    #[test]
    fn selected_slot_errors_do_not_fall_back_to_caller_values() {
        use systasis::app_container::Error;
        let outside = String::from("outside");
        let _: &dyn IValue = &outside;
        let Ok(container) = systasis::systasis_container! {
            register_value!(String::from("selected"): String as IValue);
            register_type_with!(String as IOutput, try || -> Result<String, Error> {
                try_resolve!(IValue)
            });
        }
        .build();
        let guard = container.try_resolve_i_value_ref().unwrap();
        assert!(matches!(
            container.try_resolve_i_output(),
            Err(Error::ValueAccessContention)
        ));
        drop(guard);
        assert_eq!(container.try_resolve_i_output().unwrap(), "selected");
        assert!(matches!(
            container.try_resolve_i_output(),
            Err(Error::ValueAlreadyConsumed)
        ));
        assert_eq!(outside, "outside");
    }

    trait IOutput {}
    impl IOutput for String {}
}

#[systasis::container]
#[test]
fn local_and_named_queries_ignore_initializer_type_shadowing() {
    let Ok(container) = systasis::systasis_container! {
        register_value!(7: u32 as IValue);
        register_value!(11: u32 as IValue in named);
        register_value!({
            type IValue = bool;
            let _: IValue = false;
            let value: registered_type!(IValue) = resolve!(IValue);
            value + resolve_from!(IValue, named)
        }: u32 as IOutput);
    }
    .build();
    assert_eq!(container.resolve_i_output(), 18);
}

mod leaf {
    pub trait IValue {
        fn number(&self) -> u32;
    }
    impl IValue for u32 {
        fn number(&self) -> u32 {
            *self
        }
    }
    #[systasis::container]
    pub fn run(value: u32, call: impl FnOnce(&AppContainer)) {
        let Ok(container) = systasis::systasis_container! {
            register_value!(value: u32 as dyn IValue);
        }
        .build();
        call(container);
    }
}

mod middle {
    use super::leaf;
    #[systasis::container]
    pub fn run<'a>(
        primary: &'a leaf::AppContainer,
        replica: &'a leaf::AppContainer,
        call: impl FnOnce(&AppContainer<'a>),
    ) {
        let Ok(container) = systasis::systasis_container! {
            register_container!(primary: &'a leaf::AppContainer);
            register_container!(replica: &'a leaf::AppContainer);
        }
        .build();
        call(container);
    }
}

mod outer {
    use super::{IOutput, leaf, middle};
    // Deliberately unrelated; the child's IValue is never imported here.
    trait IValue {}
    struct Outside;
    impl IValue for Outside {}
    #[allow(non_snake_case)]
    fn IValue() -> Outside {
        Outside
    }

    #[systasis::container]
    pub fn run<'a>(primary: &leaf::AppContainer, branch: &middle::AppContainer<'a>) {
        let _: &dyn IValue = &IValue();
        let Ok(container) = systasis::systasis_container! {
            register_container!(primary: &leaf::AppContainer);
            register_container!(branch: &middle::AppContainer<'a>);
            register_value!({
                let direct: resolve_type_from!(IValue, primary) = resolve_from!(IValue, primary);
                let nested: resolve_type_from!(IValue, branch::primary) = resolve_from!(IValue, branch::primary);
                let sibling = resolve_from!(IValue, branch::replica);
                let dynamic = resolve_dyn_ref_from!(IValue, branch::replica).number();
                direct + nested + sibling + dynamic
            }: u32 as IOutput);
        }.build();
        assert_eq!(container.resolve_i_output(), 7 + 7 + 11 + 11);
    }
}

#[test]
fn children_and_nested_queries_ignore_unrelated_traits_and_functions() {
    leaf::run(7, |primary| {
        leaf::run(11, |replica| {
            middle::run(primary, replica, |branch| outer::run(primary, branch));
        })
    });
}

#[test]
#[cfg(not(miri))]
fn missing_registration_never_falls_back_to_external_trait() {
    use std::{fs, path::Path, process::Command};
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let target = root
        .join("target/resolver-scope")
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
    for (name, query, diagnostic) in [
        ("local", "resolve!(IMissing)", "unregistered dependency"),
        ("alias", "resolve!(Renamed)", "unregistered dependency"),
        ("child", "resolve_from!(IMissing, primary)", "E0277"),
    ] {
        let registration = if name == "alias" {
            "register_value!(3: u32 as IMissing);"
        } else {
            ""
        };
        let source = format!(
            r#"
trait IMissing {{}} impl IMissing for u32 {{}}
use IMissing as Renamed;
trait IOutput {{}} impl IOutput for u32 {{}}
mod leaf {{
    #[systasis::container]
    pub fn run(call: impl FnOnce(&AppContainer)) {{
        let Ok(container) = systasis::systasis_container! {{}}.build();
        call(container);
    }}
}}
#[systasis::container]
fn outer(primary: &leaf::AppContainer) {{
    let Ok(_) = systasis::systasis_container! {{
        register_container!(primary: &leaf::AppContainer);
        {registration}
        register_value!({query}: u32 as IOutput);
    }}.build();
}}
fn main() {{ leaf::run(outer); }}
"#
        );
        let path = target.join(format!("{name}.rs"));
        fs::write(&path, &source).unwrap();
        let output = artifacts
            .rustc()
            .args(["--edition=2024", "--emit=metadata", "--error-format=short"])
            .arg(path)
            .arg("--out-dir")
            .arg(&target)
            .output()
            .unwrap();
        let stderr = String::from_utf8_lossy(&output.stderr);
        fs::write(target.join(format!("{name}.stderr")), &output.stderr).unwrap();
        assert!(
            !output.status.success() && stderr.contains(diagnostic),
            "{name}: {stderr}"
        );
        let error_line = if name == "child" {
            // rustc versions abbreviate this same failed scope bound differently.
            assert!(
                [
                    "the trait bound `__SystasisScope<'_, ..., ...>: Resolve<'_, ..., ..., ...>` is not satisfied",
                    "the trait bound `__SystasisScope<'_, _, Empty>: Resolve<'_, Here, _, Owned>` is not satisfied",
                ].iter().any(|bound| stderr.contains(bound)),
                "{stderr}"
            );
            source
                .lines()
                .position(|line| line == "#[systasis::container]")
                .unwrap()
                + 1
        } else {
            source
                .lines()
                .position(|line| line.contains(query))
                .unwrap()
                + 1
        };
        assert!(
            stderr.contains(&format!("{name}.rs:{error_line}:")),
            "{stderr}"
        );
    }
}
