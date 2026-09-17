//! The builder phase retains ownership and borrowing until build or abandonment.
#![cfg(not(miri))]
#![forbid(unsafe_code)]

mod support;

use std::{fs, path::PathBuf, process::Command};

#[test]
fn builder_lifecycle_diagnostics() {
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
        .join("target/builder-contracts")
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
    let artifacts = support::Artifacts::build(&mut build);

    let cases = [
        (
            "held_builder_moves_then_builds",
            r#"
fn inspect(container: &AppContainer) -> usize { container.resolve_i_value() }
#[systasis::container]
fn configure() {
    let input: String = String::from("pending");
    let builder = systasis::systasis_container! {
        register_value!(input.len(): usize as IValue);
    };
    let moved = builder;
    let Ok(container) = moved.build();
    assert_eq!(inspect(container), 7);
    assert_eq!(container.resolve_i_value(), 7);
}
fn main() { configure(); }
"#,
            None,
        ),
        (
            "dropping_builder_releases_input_borrow",
            r#"
#[systasis::container]
fn configure() {
    let mut input: String = String::from("pending");
    let builder = systasis::systasis_container! {
        register_value!(input.len(): usize as IValue);
    };
    drop(builder);
    input.push('!');
    assert_eq!(input, "pending!");
}
fn main() { configure(); }
"#,
            None,
        ),
        (
            "builder_has_no_resolver",
            r#"
#[systasis::container]
fn configure() {
    let builder = systasis::systasis_container! {
        register_value!(7_usize: usize as IValue);
    };
    builder.resolve_i_value();
}
fn main() { configure(); }
"#,
            Some(("E0599", "resolve_i_value")),
        ),
        (
            "consumed_builder_cannot_build_again",
            r#"
#[systasis::container]
fn configure() {
    let builder = systasis::systasis_container! {
        register_value!(7_usize: usize as IValue);
    };
    let first = builder.build::<()>().unwrap();
    let second = builder.build::<()>().unwrap();
    assert_eq!(first.resolve_i_value(), second.resolve_i_value());
}
fn main() { configure(); }
"#,
            Some(("E0382", "builder")),
        ),
        (
            "built_container_has_no_build",
            r#"
#[systasis::container]
fn configure() {
    let builder = systasis::systasis_container! {
        register_value!(7_usize: usize as IValue);
    };
    let Ok(container) = builder.build();
    container.build();
}
fn main() { configure(); }
"#,
            Some(("E0599", "build")),
        ),
        (
            "built_container_has_no_registration",
            r#"
#[systasis::container]
fn configure() {
    let builder = systasis::systasis_container! {
        register_value!(7_usize: usize as IValue);
    };
    let Ok(container) = builder.build();
    container.register_value(8_usize);
}
fn main() { configure(); }
"#,
            Some(("E0599", "register_value")),
        ),
        (
            "container_reference_cannot_escape_owner",
            r#"
#[systasis::container]
fn configure() -> &'static AppContainer {
    let builder = systasis::systasis_container! {
        register_value!(7_usize: usize as IValue);
    };
    let Ok(container) = builder.build();
    container
}
fn main() { let _ = configure(); }
"#,
            Some(("E0515", "cannot return value referencing")),
        ),
        (
            "pending_shared_borrow_prevents_mutation",
            r#"
#[systasis::container]
fn configure() {
    let mut input: String = String::from("pending");
    let builder = systasis::systasis_container! {
        register_value!(input.len(): usize as IValue);
    };
    input.push('!');
    drop(builder);
}
fn main() { configure(); }
"#,
            Some(("E0502", "input")),
        ),
        (
            "pending_mutable_borrow_prevents_second_mutation",
            r#"
#[systasis::container]
fn configure() {
    let mut input: String = String::from("pending");
    let builder = systasis::systasis_container! {
        register_value!({ input.push('!'); input.len() }: usize as IValue);
    };
    input.push('?');
    drop(builder);
}
fn main() { configure(); }
"#,
            Some(("E0499", "input")),
        ),
    ];

    for (name, body, expected) in cases {
        let source = target.join(format!("{name}.rs"));
        let executable = target.join(name);
        fs::write(
            &source,
            format!(
                "#![forbid(unsafe_code)]\ntrait IValue {{}}\nimpl IValue for usize {{}}\n{body}"
            ),
        )
        .expect("write builder fixture");
        let output = artifacts
            .rustc()
            .args([
                "--edition=2024",
                "--color=never",
                "--crate-name",
                "builder_fixture",
            ])
            .arg(&source)
            .arg("-o")
            .arg(&executable)
            .output()
            .expect("compile builder fixture");
        let diagnostics = String::from_utf8_lossy(&output.stderr);
        fs::write(target.join(format!("{name}.log")), diagnostics.as_bytes())
            .expect("retain builder diagnostics");
        if let Some((code, fragment)) = expected {
            assert!(!output.status.success(), "{name}: invalid fixture compiled");
            assert!(
                diagnostics.lines().any(|line| {
                    line.strip_prefix(&format!("error[{code}]:"))
                        .is_some_and(|message| message.contains(fragment))
                }),
                "{name}: expected {code} mentioning {fragment}:\n{diagnostics}"
            );
        } else {
            assert!(output.status.success(), "{name}:\n{diagnostics}");
            let execution = Command::new(&executable)
                .output()
                .expect("run positive builder fixture");
            assert!(
                execution.status.success(),
                "{name}: runtime check failed:\n{}",
                String::from_utf8_lossy(&execution.stderr)
            );
        }
    }
}
