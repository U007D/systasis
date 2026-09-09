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
    let configuration = if cfg!(feature = "resolve_unchecked") {
        "unchecked"
    } else {
        "checked"
    };
    let target = root
        .join("target/container-contracts")
        .join(backend)
        .join(configuration);
    let mut build = Command::new(env!("CARGO"));
    build
        .current_dir(&root)
        .args(["build", "--lib", "--offline", "--locked", "--target-dir"])
        .arg(&target);
    if !cfg!(feature = "std") {
        build.arg("--no-default-features");
    }
    if cfg!(feature = "resolve_unchecked") {
        build.args(["--features", "resolve_unchecked"]);
    }
    let output = build.output().expect("build library");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let cases = [
        #[cfg(feature = "resolve_unchecked")]
        (
            "unchecked_method_requires_unsafe",
            "",
            "register_value!(String::new(): String as IValue);",
            "built.unwrap().resolve_i_value_unchecked();",
            Some("E0133"),
        ),
        #[cfg(feature = "resolve_unchecked")]
        (
            "unchecked_query_requires_unsafe",
            "",
            "register_value!(String::new(): String as IValue); register_value!(resolve_unchecked!(IValue).len(): usize as ISize);",
            "",
            Some("E0133"),
        ),
        #[cfg(feature = "resolve_unchecked")]
        (
            "unchecked_cannot_restore_borrowed_owned_accessor",
            "",
            "register_value!(String::new(): String as IValue); register_type_with!(usize as ISize, || unsafe { resolve_ref_unchecked!(IValue) }.len());",
            "unsafe { built.unwrap().resolve_i_value_unchecked(); }",
            Some("E0599"),
        ),
        #[cfg(feature = "resolve_unchecked")]
        (
            "copy_has_no_unchecked_accessor",
            "",
            "register_value!(1: u32 as INumber);",
            "unsafe { built.unwrap().resolve_i_number_unchecked(); }",
            Some("E0599"),
        ),
        #[cfg(feature = "resolve_unchecked")]
        (
            "fresh_has_no_unchecked_accessor",
            "",
            "register_type!(String as IValue);",
            "unsafe { built.unwrap().resolve_i_value_unchecked(); }",
            Some("E0599"),
        ),
        #[cfg(not(feature = "resolve_unchecked"))]
        (
            "unchecked_accessor_is_feature_gated",
            "",
            "register_value!(String::new(): String as IValue);",
            "unsafe { built.unwrap().resolve_i_value_unchecked(); }",
            Some("E0599"),
        ),
        (
            "named_registration_is_not_default_dependency",
            "",
            "register_value!(String::new(): String as IValue in test); register_value!(try_resolve!(IValue)?: String as a::IValue);",
            "",
            Some("unregistered dependency"),
        ),
        (
            "named_type_query_does_not_fall_back_to_default",
            "",
            "register_value!(String::new(): String as IValue); register_value!(String::new(): resolve_type_from!(IValue, test) as a::IValue);",
            "",
            Some("unregistered type lookup"),
        ),
        (
            "named_dyn_query_requires_matching_opt_in",
            "",
            "register_value!(String::new(): String as dyn IValue); register_value!(String::new(): String as IValue in test); register_value!({ let _: Option<&resolve_type_from!(dyn IValue, test)> = None; 1 }: u32 as INumber);",
            "",
            Some("dyn type lookup requires an as dyn registration"),
        ),
        (
            "namespace_borrow_excludes_only_matching_owned_accessor",
            "",
            "register_value!(String::new(): String as IValue in test); register_value!(String::new(): String as IValue); register_type_with!(usize as ISize, try || -> Result<usize, systasis::app_container::Error> { Ok(try_resolve_ref_from!(IValue, test)?.len()) });",
            "built.unwrap().try_resolve_i_value_in_test();",
            Some("E0599"),
        ),
        (
            "namespace_borrow_preserves_other_namespace_owned_accessor",
            "",
            "register_value!(String::new(): String as IValue in test); register_value!(String::new(): String as IValue); register_type_with!(usize as ISize, try || -> Result<usize, systasis::app_container::Error> { Ok(try_resolve_ref_from!(IValue, test)?.len()) });",
            "built.unwrap().try_resolve_i_value().unwrap();",
            None,
        ),
        (
            "named_registration_has_no_unsuffixed_accessor",
            "",
            "register_value!(String::new(): String as IValue in test);",
            "built.unwrap().try_resolve_i_value();",
            Some("E0599"),
        ),
        (
            "namespace_cycle_is_rejected",
            "",
            "register_value!(try_resolve_from!(IValue, two)?: String as IValue in one); register_value!(try_resolve_from!(IValue, one)?: String as IValue in two);",
            "",
            Some("registration dependency cycle"),
        ),
        (
            "namespace_method_name_collision",
            "",
            "register_value!(String::new(): String as IValue in test); register_value!(String::new(): String as IValueInTest);",
            "",
            Some("interfaces generate the same resolver name: resolve_i_value_in_test"),
        ),
        (
            "default_alias_method_name_collision",
            "",
            "register_value!(String::new(): String as IValue); register_value!(String::new(): String as IValueInDefault);",
            "",
            Some("interfaces generate the same resolver name: resolve_i_value_in_default"),
        ),
        (
            "operation_modifier_method_name_collision",
            "",
            "register_value!(String::new(): String as IValue); register_value!(String::new(): String as IValueRef);",
            "",
            Some("interfaces generate the same resolver name: resolve_i_value_ref"),
        ),
        (
            "combined_dyn_type_requires_group_opt_in",
            "",
            "register_value!(String::new(): String as IValue + a::IValue); register_value!({ let _: Option<&resolve_type!(dyn a::IValue + IValue)> = None; 1 }: u32 as INumber);",
            "",
            Some("dyn type lookup requires an as dyn registration"),
        ),
        (
            "combined_dyn_member_query_is_not_whole_group",
            "",
            "register_value!(String::new(): String as dyn IValue + a::IValue); register_value!({ let _ = try_resolve_dyn_ref!(IValue)?; 1 }: u32 as INumber);",
            "",
            Some("unregistered dependency"),
        ),
        (
            "combined_dyn_constructor_borrow_removes_owned_accessor",
            "",
            "register_value!(String::new(): String as dyn IValue + a::IValue); register_type_with!(u32 as INumber, try || -> Result<u32, systasis::app_container::Error> { let _ = try_resolve_dyn_ref!(a::IValue + IValue)?; Ok(1) });",
            "built.unwrap().try_resolve_i_value_i_value();",
            Some("E0599"),
        ),
        (
            "generic_wrapper_needs_whole_type_copy_bound",
            "",
            "",
            "mod generic { trait IValue {} #[derive(Clone, Copy)] struct Wrapper<T>(T); impl<T> IValue for Wrapper<T> {} #[systasis::container] fn run<T: Copy>(value: Wrapper<T>) { let Ok(container) = systasis::systasis_container! { register_value!(value: Wrapper<T> as IValue); }.build(); } }",
            Some(
                "generic registration: Copy is known indirectly; add an explicit Copy bound on the registered type",
            ),
        ),
        (
            "generic_supertrait_needs_explicit_copy_bound",
            "",
            "",
            "mod generic { trait IValue: Copy {} #[systasis::container] fn run<T: IValue>(value: T) { let Ok(container) = systasis::systasis_container! { register_value!(value: T as IValue); }.build(); } }",
            Some(
                "generic registration: Copy is known indirectly; add an explicit Copy bound on the registered type",
            ),
        ),
        (
            "generic_unbounded_has_no_copy_accessor",
            "",
            "",
            "mod generic { trait IValue {} impl<T> IValue for T {} #[systasis::container] fn run<T>(value: T) { let Ok(container) = systasis::systasis_container! { register_value!(value: T as IValue); }.build(); container.resolve_i_value(); } }",
            Some("E0599"),
        ),
        (
            "generic_copy_has_no_take_accessor",
            "",
            "",
            "mod generic { trait IValue {} impl<T> IValue for T {} #[systasis::container] fn run<T: Copy>(value: T) { let Ok(container) = systasis::systasis_container! { register_value!(value: T as IValue); }.build(); container.try_resolve_i_value(); } }",
            Some("E0599"),
        ),
        (
            "generic_registered_interface_bound_is_checked",
            "",
            "",
            "mod generic { trait IValue {} #[systasis::container] fn run<T>(value: T) { let Ok(container) = systasis::systasis_container! { register_value!(value: T as IValue); }.build(); } }",
            Some("E0277"),
        ),
        (
            "generic_stored_value_requires_send_bound",
            "",
            "",
            "mod generic { trait IValue {} impl<T> IValue for T {} #[systasis::container(require(Send))] fn run<T>(value: T) { let Ok(container) = systasis::systasis_container! { register_value!(value: T as IValue); }.build(); } }",
            Some("E0277"),
        ),
        (
            "generic_capture_requires_sync_bound",
            "",
            "",
            "mod generic { trait IValue {} impl<T> IValue for T {} #[systasis::container(require(Sync))] fn run<T: Clone>(value: T) { let Ok(container) = systasis::systasis_container! { register_type_with!(T as IValue, || value.clone()); }.build(); } }",
            Some("E0277"),
        ),
        (
            "known_copy_requires_explicit_policy",
            "",
            "",
            "fn check<T: Copy>() { use systasis::__private::DetectCopy as _; systasis::__private::verify_generic_fallback((&&systasis::__private::Pick::<T>::NEW).evidence()); } check::<u32>();",
            Some(
                "generic registration: Copy is known indirectly; add an explicit Copy bound on the registered type",
            ),
        ),
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
            "combined_dyn_requires_every_trait_to_be_dyn_safe_even_if_unused",
            "",
            "register_value!(String::new(): String as dyn IValue + IGeneric);",
            "",
            Some("E0038"),
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
trait IValueInTest {{}} impl IValueInTest for String {{}}
trait IValueInDefault {{}} impl IValueInDefault for String {{}}
trait IValueRef {{}} impl IValueRef for String {{}}
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
