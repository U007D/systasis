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
            "group_has_no_individual_accessor",
            "",
            "register_value!(String::new(): String as IValue + a::IValue);",
            "built.unwrap().try_resolve_i_value();",
            Some("E0599"),
        ),
        (
            "group_member_is_not_dependency",
            "",
            "register_value!(String::new(): String as IValue + a::IValue); register_value!(try_resolve!(IValue)?: String as b::IValue);",
            "",
            Some("unregistered dependency"),
        ),
        (
            "group_member_is_not_type_lookup",
            "",
            "register_value!(String::new(): String as IValue + a::IValue); register_value!(String::new(): registered_type!(IValue) as b::IValue);",
            "",
            Some("unregistered type lookup"),
        ),
        (
            "all_group_traits_are_checked",
            "",
            "register_value!(String::new(): String as IValue + INumber);",
            "",
            Some("E0277"),
        ),
        (
            "combined_dyn_is_not_yet_implemented",
            "",
            "register_value!(String::new(): String as dyn IValue + a::IValue);",
            "",
            Some("combined dyn trait accessors are not implemented yet"),
        ),
        (
            "normalized_group_override",
            "",
            "register_value!(unknown!(): MissingType as a::IValue + IValue); register_value!(String::new(): String as IValue + a::IValue);",
            "built.unwrap().try_resolve_i_value_i_value().unwrap();",
            None,
        ),
        (
            "dyn_type_query_requires_opt_in",
            "",
            "register_value!(String::new(): String as IValue); register_value!({ let _: Option<&resolve_type!(dyn IValue)> = None; 1 }: u32 as INumber);",
            "",
            Some("dyn type lookup requires an as dyn registration"),
        ),
        (
            "dyn_type_query_requires_registration",
            "",
            "register_value!({ let _: Option<&resolve_type!(dyn IValue)> = None; 1 }: u32 as INumber);",
            "",
            Some("unregistered type lookup"),
        ),
        (
            "dyn_query_requires_opt_in",
            "",
            "register_value!(String::new(): String as IValue); register_value!({ let _guard = try_resolve_dyn_ref!(IValue)?; 1 }: u32 as INumber);",
            "",
            Some("requested resolver is unavailable"),
        ),
        (
            "dyn_constructor_borrow_removes_owned_accessor",
            "",
            "register_value!(String::new(): String as dyn IValue); register_type_with!(u32 as INumber, try || -> Result<u32, systasis::app_container::Error> { let _guard = try_resolve_dyn_ref!(IValue)?; Ok(1) });",
            "built.unwrap().try_resolve_i_value();",
            Some("E0599"),
        ),
        (
            "constructor_borrow_removes_owned_accessor",
            "",
            "register_value!(String::new(): String as IValue); register_type_with!(u32 as INumber, try || -> Result<u32, systasis::app_container::Error> { Ok(try_resolve_ref!(IValue)?.len() as u32) });",
            "built.unwrap().try_resolve_i_value();",
            Some("E0599"),
        ),
        (
            "constructor_borrow_prevents_build_consumption",
            "",
            "register_value!(String::new(): String as IValue); register_type_with!(u32 as INumber, try || -> Result<u32, systasis::app_container::Error> { Ok(try_resolve_ref!(IValue)?.len() as u32) }); register_value!(try_resolve!(IValue)?.len() as usize: usize as ISize);",
            "",
            Some("requested resolver is unavailable"),
        ),
        (
            "untyped_constructor_capture",
            "",
            "register_type_with!(String as IValue, move || untyped.clone());",
            "",
            Some("captured constructor bindings require an explicit type annotation"),
        ),
        (
            "capture_is_moved",
            "",
            "register_type_with!(String as IValue, move || config.clone());",
            "drop(config);",
            Some("E0382"),
        ),
        (
            "constructor_cannot_consume_capture",
            "",
            "register_type_with!(String as IValue, move || config);",
            "",
            Some("E0507"),
        ),
        (
            "fallible_constructor_requires_annotation",
            "",
            "register_type_with!(String as IValue, try || None);",
            "",
            Some("fallible constructor requires an explicit return type"),
        ),
        (
            "wrong_fallible_output",
            "",
            "register_type_with!(String as IValue, try || -> Option<u32> { Some(1) });",
            "",
            Some("E0271"),
        ),
        (
            "factory_has_no_clone",
            "",
            "register_type_with!(String as IValue, || String::new());",
            "built.unwrap().try_resolve_i_value_clone();",
            Some("E0599"),
        ),
        (
            "capture_controls_send",
            "require(Send)",
            "register_type_with!(String as IValue, move || local_config.as_ref().clone());",
            "",
            Some("E0277"),
        ),
        (
            "non_dyn_safe_opt_in",
            "",
            "register_value!(String::new(): String as dyn IGeneric);",
            "",
            Some("E0038"),
        ),
        (
            "static_registration_has_no_dyn_method",
            "",
            "register_value!(String::new(): String as IValue);",
            "built.unwrap().try_resolve_i_value_dyn_ref();",
            Some("E0599"),
        ),
        (
            "wrong_lifetime_interface",
            "",
            "register_value!(Borrowed(\"value\"): Borrowed<'_> as IValue);",
            "",
            Some("E0277"),
        ),
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
trait IGeneric {{ fn generic<T>(&self); }}
impl IGeneric for String {{ fn generic<T>(&self) {{}} }}
struct Borrowed<'a>(&'a str);
impl IValue for String {{}}
trait INumber {{}}
trait ISize {{}} impl ISize for usize {{}}
impl INumber for u32 {{}}
mod a {{ pub trait IValue {{}} impl IValue for String {{}} }}
mod b {{ pub trait IValue {{}} impl IValue for String {{}} }}
#[systasis::container({requirements})]
fn main() {{
    let config: String = String::from("config");
    let untyped = String::from("untyped");
    let local_config: std::rc::Rc<String> = std::rc::Rc::new(String::from("local"));
    let built = systasis::systasis_container! {{ {registrations} }}.build::<systasis::app_container::Error>();
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
