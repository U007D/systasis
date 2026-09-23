//! Artifact discovery is independent of Cargo's on-disk layout and toolchain.
#![cfg(not(miri))]
#![forbid(unsafe_code)]

mod support;

use support::Artifacts;

fn arguments(messages: &str) -> Vec<String> {
    Artifacts::from_messages(messages)
        .rustc()
        .get_args()
        .map(|argument| argument.to_str().unwrap().to_owned())
        .collect()
}

#[test]
fn stable_layout_selects_hashed_metadata_and_dependency_directory() {
    assert_eq!(
        arguments(
            r#"{"filenames":["/target/debug/libsystasis.rlib","/target/debug/deps/libsystasis-123.rmeta"]}"#
        ),
        [
            "--extern",
            "systasis=/target/debug/libsystasis.rlib",
            "--extern",
            "systasis=/target/debug/deps/libsystasis-123.rmeta",
            "-L",
            "dependency=/target/debug",
            "-L",
            "dependency=/target/debug/deps"
        ]
    );
}

#[test]
fn split_metadata_layout_uses_all_reported_artifact_directories() {
    let args = arguments(
        r#"
{"filenames":["/target/debug/build/dependency/456/out/libdependency-456.rlib","/target/debug/build/dependency/456/out/libdependency-456.rmeta"]}
{"filenames":["/target/debug/build/macros/789/out/libmacros-789.dylib"]}
{"filenames":["/target/debug/libsystasis.rlib","/target/debug/build/systasis/123/out/libsystasis-123.rmeta"]}
{"reason":"build-finished","success":true}
"#,
    );
    assert_eq!(
        args[3],
        "systasis=/target/debug/build/systasis/123/out/libsystasis-123.rmeta"
    );
    for directory in ["dependency/456/out", "macros/789/out", "systasis/123/out"] {
        assert!(args.contains(&format!("dependency=/target/debug/build/{directory}")));
    }
    assert_eq!(args.len(), 12);
}

#[test]
fn paths_decode_json_escapes_without_treating_strings_or_nested_keys_as_artifacts() {
    let args = arguments(
        r#"
{"message":"\"filenames\":[\"libsystasis-wrong.rmeta\"]","nested":{"filenames":["libsystasis-nested.rmeta"]}}
{"message":"filenames"}
{"filenames":["/path with ,\"quotes\"/caf\u00e9-\ud83d\ude80/libsystasis-123.rmeta","/path with ,\"quotes\"/caf\u00e9-\ud83d\ude80/libsystasis-123.rlib"]}
"#,
    );
    assert_eq!(
        args[3],
        "systasis=/path with ,\"quotes\"/café-🚀/libsystasis-123.rmeta"
    );
}

#[test]
#[should_panic(expected = "expected one systasis metadata artifact")]
fn missing_metadata_is_a_driver_failure() {
    let _ = arguments(r#"{"filenames":["/target/debug/libsystasis.rlib"]}"#);
}

#[test]
#[should_panic(expected = "expected one systasis metadata artifact")]
fn ambiguous_metadata_is_a_driver_failure() {
    let _ =
        arguments(r#"{"filenames":["/target/libsystasis-a.rmeta","/target/libsystasis-b.rmeta"]}"#);
}

#[test]
#[should_panic(expected = "expected one systasis library artifact")]
fn missing_linkable_library_is_a_driver_failure() {
    let _ = arguments(r#"{"filenames":["/target/debug/libsystasis-123.rmeta"]}"#);
}

#[test]
fn invalid_filename_strings_are_rejected() {
    for messages in [
        r#"{"filenames":["\x"]}"#,
        r#"{"filenames":["\ud800"]}"#,
        r#"{"filenames":["\ud800\u0041"]}"#,
        r#"{"filenames":["\udfff"]}"#,
        r#"{"filenames":["\u123"]}"#,
        r#"{"filenames":["unterminated]}"#,
        r#"{"filenames":[null]}"#,
    ] {
        let panic = std::panic::catch_unwind(|| arguments(messages)).expect_err(messages);
        let message = panic
            .downcast_ref::<String>()
            .expect("formatted driver failure");
        assert!(
            message.starts_with("valid Cargo artifact filenames:"),
            "{message}"
        );
    }
}

// This small independent package allows validating artifact handling on stable
// even when the production proc-macro package temporarily requires nightly.
#[test]
fn independent_cargo_package_supports_metadata_checks_linking_and_intended_errors() {
    use std::{fs, path::Path, process::Command};
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let base = root
        .join("target/compiler-support-independent")
        .join(if cfg!(feature = "std") {
            "std"
        } else {
            "no-std"
        });
    let fixture = base.join("package");
    let target = base.join("artifacts");
    fs::create_dir_all(fixture.join("src")).unwrap();
    fs::write(
        fixture.join("Cargo.toml"),
        "[package]\nname = \"systasis\"\nversion = \"0.0.0\"\nedition = \"2024\"\n[workspace]\n",
    )
    .unwrap();
    fs::write(
        fixture.join("src/lib.rs"),
        "pub fn number() -> u32 { 42 }\n",
    )
    .unwrap();
    let artifacts = Artifacts::build(
        Command::new(env!("CARGO"))
            .current_dir(&fixture)
            .args(["build", "--lib", "--offline", "--target-dir"])
            .arg(&target),
    );
    let source = target.join("consumer.rs");
    fs::write(&source, "fn main() { assert_eq!(systasis::number(), 42); }").unwrap();
    let compile = |extra: &[&str]| {
        artifacts
            .rustc()
            .args(["--edition=2024", "--out-dir"])
            .arg(&target)
            .arg(&source)
            .args(extra)
            .output()
            .unwrap()
    };
    for extra in [&["--emit=metadata"][..], &[][..]] {
        let output = compile(extra);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    assert!(
        Command::new(target.join("consumer"))
            .status()
            .unwrap()
            .success()
    );
    fs::write(&source, "fn main() { let _: &str = systasis::number(); }").unwrap();
    let output = compile(&["--error-format=json", "--emit=metadata"]);
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("\"code\":\"E0308\""),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn actual_library_artifacts_support_metadata_checks_linking_and_intended_errors() {
    use std::{fs, path::Path, process::Command};
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let target = root
        .join("target/compiler-support")
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
    let artifacts = Artifacts::build(&mut build);
    let source = target.join("consumer.rs");
    fs::write(&source, "fn main() { let slot = systasis::__private::TakeSlot::new(String::from(\"value\")); assert_eq!(slot.try_resolve().unwrap(), \"value\"); }").unwrap();
    let compile = |extra: &[&str]| {
        artifacts
            .rustc()
            .args(["--edition=2024", "--out-dir"])
            .arg(&target)
            .arg(&source)
            .args(extra)
            .output()
            .unwrap()
    };
    for extra in [&["--emit=metadata"][..], &[][..]] {
        let output = compile(extra);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    assert!(
        Command::new(target.join("consumer"))
            .status()
            .unwrap()
            .success()
    );
    fs::write(
        &source,
        "fn main() { let _: u32 = systasis::container::Error::ValueAlreadyConsumed; }",
    )
    .unwrap();
    let output = compile(&["--error-format=json", "--emit=metadata"]);
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("\"code\":\"E0308\""),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
