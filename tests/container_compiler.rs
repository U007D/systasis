//! Compile downstream code and match errors, not merely unsuccessful processes.
#![cfg(not(miri))]
#![forbid(unsafe_code)]

use std::{fs, path::PathBuf, process::Command};

#[test]
fn container_diagnostics() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let backend = if cfg!(feature = "std") {
        "std"
    } else {
        "no-std"
    };
    let target = root.join("target/container-contracts").join(backend);
    let mut build = Command::new(env!("CARGO"));
    build
        .current_dir(&root)
        .args(["build", "--lib", "--offline", "--locked", "--target-dir"])
        .arg(&target);
    if !cfg!(feature = "std") {
        build.arg("--no-default-features");
    }
    let output = build.output().expect("build library");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let cases = [
        (
            "baseline",
            "",
            "register_value!(String::new(): String as IValue);",
            "",
            None,
        ),
        (
            "missing_dependency",
            "",
            "register_value!(try_resolve!(IMissing)?: String as IValue);",
            "",
            Some("unregistered dependency"),
        ),
        (
            "missing_type",
            "",
            "register_value!(String::new(): registered_type!(IMissing) as IValue);",
            "",
            Some("unregistered type lookup"),
        ),
        (
            "type_cycle",
            "",
            "register_value!(String::new(): registered_type!(IValue) as IValue);",
            "",
            Some("registered type lookup cycle"),
        ),
        (
            "value_cycle",
            "",
            "register_value!(try_resolve!(IValue)?: String as IValue);",
            "",
            Some("registration dependency cycle: IValue -> IValue"),
        ),
        (
            "wrong_interface",
            "",
            "register_value!(42_u32: u32 as IValue);",
            "",
            Some("E0277"),
        ),
        (
            "consumable_has_no_copy_resolver",
            "",
            "register_value!(String::new(): String as IValue);",
            "built.unwrap().resolve_i_value();",
            Some("E0599"),
        ),
        (
            "copy_has_no_take_resolver",
            "",
            "register_value!(7_u32: u32 as INumber);",
            "built.unwrap().try_resolve_i_number();",
            Some("E0599"),
        ),
        (
            "local_not_sync",
            "require(!Sync)",
            "register_value!(String::new(): String as IValue);",
            "fn check<T: Sync>(_: &T) {} check(built.unwrap());",
            Some("E0277"),
        ),
        (
            "contradictory",
            "require(Sync, !Sync)",
            "",
            "",
            Some("Sync and !Sync cannot be required together"),
        ),
        (
            "qualified_name_collision",
            "",
            "register_value!(String::new(): String as a::IValue); register_value!(String::new(): String as b::IValue);",
            "",
            Some("interfaces generate the same resolver name"),
        ),
        (
            "no_register_after_build",
            "",
            "",
            "built.unwrap().register_value();",
            Some("E0599"),
        ),
        (
            "backing_cannot_escape",
            "",
            "register_value!(String::new(): String as IValue);",
            "let _: systasis::Ref<'static, String> = built.unwrap().try_resolve_i_value_ref().unwrap();",
            Some("E0716"),
        ),
    ];
    for (name, requirements, registrations, after, expected) in cases {
        let source = format!(
            r#"
trait IValue {{}}
impl IValue for String {{}}
trait INumber {{}}
impl INumber for u32 {{}}
mod a {{ pub trait IValue {{}} impl IValue for String {{}} }}
mod b {{ pub trait IValue {{}} impl IValue for String {{}} }}
#[systasis::container({requirements})]
fn main() {{
    let built = systasis_container! {{ {registrations} }}.build::<systasis::app_container::Error>();
    {after}
}}
"#
        );
        let path = target.join(format!("{name}.rs"));
        fs::write(&path, source).expect("write compiler fixture");
        let output = Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata", "--error-format=json"])
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
            .expect("compile fixture");
        let diagnostics = String::from_utf8_lossy(&output.stderr);
        fs::write(target.join(format!("{name}.jsonl")), &output.stderr)
            .expect("retain diagnostics");
        if let Some(expected) = expected {
            assert!(!output.status.success(), "{name} unexpectedly compiled");
            assert!(
                diagnostics
                    .lines()
                    .any(|line| line.contains("\"level\":\"error\"")
                        && (line.contains(&format!("\"code\":\"{expected}\""))
                            || line.contains(&format!("\"message\":\"{expected}")))),
                "{name}: missing {expected}: {diagnostics}"
            );
        } else {
            assert!(output.status.success(), "{name}: {diagnostics}");
        }
    }
}
