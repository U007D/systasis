//! Native captures keep the concrete API across crate boundaries and retain
//! ordinary Rust ownership, lifetime, privacy, and auto-trait diagnostics.
#![cfg(not(miri))]
#![forbid(unsafe_code)]

mod support;

use std::{fs, path::Path, process::Command};

#[test]
fn native_closure_public_api_and_rejection_controls() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let backend = if cfg!(feature = "std") {
        "std"
    } else {
        "no-std"
    };
    let target = root.join("target/native-compiler").join(backend);
    let mut build = Command::new(env!("CARGO"));
    build
        .current_dir(root)
        .args(["build", "--lib", "--offline", "--locked", "--target-dir"])
        .arg(&target);
    if !cfg!(feature = "std") {
        build.arg("--no-default-features");
    }
    let artifacts = support::Artifacts::build(&mut build);
    let provider = artifacts
        .rustc()
        .args([
            "--edition=2024",
            "--crate-type=rlib",
            "--crate-name=native_provider",
            "-Dwarnings",
            "--out-dir",
        ])
        .arg(&target)
        .arg(root.join("tests/fixtures/native_provider.rs"))
        .output()
        .expect("compile native provider");
    assert!(
        provider.status.success(),
        "{}",
        String::from_utf8_lossy(&provider.stderr)
    );
    let dependency = format!(
        "native_provider={}",
        target.join("libnative_provider.rlib").display()
    );
    let binary = target.join("native_consumer");
    let consumer = artifacts
        .rustc()
        .args([
            "--edition=2024",
            "-Dwarnings",
            "--extern",
            &dependency,
            "-o",
        ])
        .arg(&binary)
        .arg(root.join("tests/fixtures/native_consumer.rs"))
        .output()
        .expect("compile native consumer");
    assert!(
        consumer.status.success(),
        "{}",
        String::from_utf8_lossy(&consumer.stderr)
    );
    assert!(
        Command::new(binary)
            .status()
            .expect("run native consumer")
            .success()
    );

    for (name, source, expected) in [
        (
            "private_result",
            r#"
            fn inspect(container: &native_provider::private_result::SystasisContainer) {
                let _ = container.resolve_i_service();
            }
            fn main() {}
        "#,
            "type `private_result::Service` is private",
        ),
        (
            "move_capture_twice",
            r#"
            trait IValue {} impl IValue for String {}
            #[systasis::container]
            fn main() {
                let value: String = String::from("owned");
                let Ok(container) = systasis::systasis_container! {
                    register_type_with!(String as IValue, move || { assert!(!value.is_empty()); value });
                }.build();
                drop(container.resolve_i_value());
            }
        "#,
            "E0507",
        ),
        (
            "non_send_capture",
            r#"
            trait IValue {} impl IValue for String {}
            #[systasis::container(require(Send))]
            fn main() {
                let value: std::rc::Rc<String> = std::rc::Rc::new(String::from("local"));
                let Ok(container) = systasis::systasis_container! {
                    register_type_with!(String as IValue, move || format!("{value}"));
                }.build();
                drop(container.resolve_i_value());
            }
        "#,
            "E0277",
        ),
        (
            "non_send_inferred_capture",
            r#"
            trait IValue {} impl IValue for String {}
            #[systasis::container(require(Send))]
            fn main() {
                let value = std::rc::Rc::new(String::from("local"));
                let Ok(container) = systasis::systasis_container! {
                    register_type_with!(String as IValue, move || value.as_ref().clone());
                }.build();
                drop(container.resolve_i_value());
            }
        "#,
            "`Rc<String>` cannot be sent between threads safely",
        ),
        (
            "non_sync_inferred_capture",
            r#"
            trait IValue {} impl IValue for u32 {}
            #[systasis::container(require(Sync))]
            fn main() {
                let value = core::cell::Cell::new(7_u32);
                let Ok(container) = systasis::systasis_container! {
                    register_type_with!(u32 as IValue, move || value.get());
                }.build();
                let _ = container.resolve_i_value();
            }
        "#,
            "`Cell<u32>` cannot be shared between threads safely",
        ),
        (
            "captured_value_moved",
            r#"
            trait IValue {} impl IValue for String {}
            #[systasis::container]
            fn main() {
                let value: String = String::from("owned");
                let Ok(container) = systasis::systasis_container! {
                    register_type_with!(String as IValue, || format!("{value}"));
                }.build();
                drop(value);
                drop(container.resolve_i_value());
            }
        "#,
            "E0382",
        ),
    ]
    .into_iter()
    .map(|(name, source, expected)| (name, source.to_owned(), expected))
    .chain(core::iter::once_with(|| {
        let source = include_str!("native_child_context.rs").replace(
            "// LOCAL_GUARD_AUTO_TRAIT_REJECTION",
            "fn assert_bound<T: Sync>(_: &T) {} assert_bound(&container);",
        );
        ("local_child_container_is_not_sync", format!("{source}\nfn main() {{}}\n"), "`Cell<()>` cannot be shared between threads safely")
    }))
    {
        let path = target.join(format!("{name}.rs"));
        fs::write(&path, source).expect("write native rejection fixture");
        let output = artifacts
            .rustc()
            .args([
                "--edition=2024",
                "--extern",
                &dependency,
                "--emit=metadata",
                "--out-dir",
            ])
            .arg(&target)
            .arg(&path)
            .output()
            .expect("compile native rejection");
        let diagnostics = String::from_utf8_lossy(&output.stderr);
        fs::write(target.join(format!("{name}.stderr")), &output.stderr)
            .expect("retain native diagnostic");
        assert!(!output.status.success(), "{name} unexpectedly compiled");
        assert!(
            diagnostics.contains(expected),
            "{name}: expected {expected}\n{diagnostics}"
        );
        if name == "local_child_container_is_not_sync" {
            assert!(
                diagnostics.split("\nerror").any(|error| {
                    error.contains("E0277")
                        && error.contains(expected)
                        && error.contains("assert_bound(&container)")
                }),
                "{name}: expected the Sync assertion to fail:\n{diagnostics}"
            );
        }
    }
}
