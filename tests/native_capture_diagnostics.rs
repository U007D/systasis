//! The local native-capture E0597 points to its remedy without rejecting
//! supported captured lifetimes, macro scopes, or unrelated short-lived inputs.
#![cfg(not(miri))]
#![forbid(unsafe_code)]

mod support;

use std::{fs, path::Path, process::Command};

#[test]
fn local_native_capture_errors_explain_the_working_rewrite() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let target = root
        .join("target/native-capture-diagnostics")
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
    for (case, error, capture_note) in [
        ("borrowed", Some("E0597"), true),
        ("inferred", Some("E0597"), true),
        // rustc does not include the bound's source note in E0521. Retain this
        // control as an explicit limit, not evidence of a tailored diagnostic.
        ("parameter", Some("E0521"), false),
        ("wrong_output", Some("E0308"), false),
        ("owned", None, false),
        ("static_reference", None, false),
        ("ignored", None, false),
        ("helper", None, false),
        ("named", None, false),
        ("generic", None, false),
    ] {
        let binary = target.join(case);
        let output = artifacts
            .rustc()
            .args([
                "--edition=2024",
                "-Dwarnings",
                "--color=never",
                "--cfg",
                case,
                "-o",
            ])
            .arg(&binary)
            .arg(root.join("tests/fixtures/native_capture_diagnostic.rs"))
            .output()
            .expect("compile capture diagnostic fixture");
        fs::write(target.join(format!("{case}.stderr")), &output.stderr)
            .expect("retain capture diagnostic");
        let diagnostics = String::from_utf8_lossy(&output.stderr);
        if let Some(code) = error {
            assert!(!output.status.success(), "{case} unexpectedly compiled");
            assert!(
                diagnostics
                    .lines()
                    .any(|line| line.starts_with(&format!("error[{code}]:"))),
                "wrong error for {case}:\n{diagnostics}"
            );
        } else {
            assert!(output.status.success(), "{case}:\n{diagnostics}");
            assert!(
                Command::new(binary)
                    .status()
                    .expect("run capture fixture")
                    .success()
            );
        }
        let remedy = "systasis cannot store this borrowed capture here; register a non-borrowing implementation.";
        assert_eq!(
            diagnostics.contains(remedy),
            capture_note,
            "{case}:\n{diagnostics}"
        );
        if capture_note {
            assert!(
                diagnostics.contains(
                    "note: requirement that the value outlives `'static` introduced here"
                ),
                "{case} lost its explanatory source note:\n{diagnostics}"
            );
        }
    }
}
