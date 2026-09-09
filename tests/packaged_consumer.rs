//! Local archive validation; never publishes or fetches registry dependencies.
//!
//! Run explicitly with:
//! `cargo +stable test --test packaged_consumer --offline -- --ignored --nocapture`
//! This stages sources, packages both crates and builds two downstream consumers,
//! so it is excluded from the fast default test run.
#![cfg(not(miri))]
#![forbid(unsafe_code)]

use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    time::{Instant, SystemTime, UNIX_EPOCH},
};

fn checked(command: &mut Command) -> Output {
    let output = command
        .output()
        .expect("execute artifact validation command");
    assert!(
        output.status.success(),
        "{command:?}\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn cargo(directory: &Path) -> Command {
    let mut command = Command::new(env!("CARGO"));
    command.current_dir(directory);
    command
}

fn toml_string(path: &Path) -> String {
    // Rust debug escaping is TOML-compatible for these ordinary filesystem paths.
    format!("{:?}", path.to_str().expect("UTF-8 workspace path"))
}

fn archive(stage: &Path, artifacts: &Path, package: &str, patch: Option<&str>) -> PathBuf {
    let mut command = cargo(stage);
    command
        .args([
            "package",
            "--offline",
            "--allow-dirty",
            "--no-verify",
            "-p",
            package,
            "--target-dir",
        ])
        .arg(artifacts);
    if let Some(patch) = patch {
        command.args(["--config", patch]);
    }
    checked(&mut command);
    let archive = artifacts
        .join("package")
        .join(format!("{package}-{}.crate", env!("CARGO_PKG_VERSION")));
    assert!(archive.is_file(), "missing {}", archive.display());
    let extracted = artifacts.join("extracted");
    fs::create_dir_all(&extracted).expect("create extraction directory");
    checked(
        Command::new("tar")
            .arg("-xzf")
            .arg(&archive)
            .arg("-C")
            .arg(&extracted),
    );
    let package_root = extracted.join(format!("{package}-{}", env!("CARGO_PKG_VERSION")));
    for license in ["LICENSE-MIT", "LICENSE-APACHE"] {
        let expected = fs::read(stage.join(license)).expect("read staged license");
        let actual =
            fs::read(package_root.join(license)).expect("both license texts must be packaged");
        assert_eq!(actual, expected, "{package}: incorrect {license}");
    }
    package_root
}

#[test]
#[ignore = "packages extracted local artifacts and compiles/runs both consumer backends"]
fn extracted_packages_build_and_run_std_and_no_std_consumers() {
    let started = Instant::now();
    let source = Path::new(env!("CARGO_MANIFEST_DIR"));
    let protected = ["Cargo.toml", "Cargo.lock", "systasis-macros/Cargo.toml"];
    let before = protected
        .map(|path| fs::read(source.join(path)).expect("snapshot source manifest/lockfile"));
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock after epoch")
        .as_nanos();
    let scratch = source
        .join("target/packaged-consumer")
        .join(format!("{}-{nonce}", std::process::id()));
    let stage = scratch.join("source");
    fs::create_dir_all(&stage).expect("create isolated source stage");

    // Copy the current tracked sources, including uncommitted tracked fixes, but
    // no Git metadata or generated build outputs. All Cargo writes stay here.
    let files = checked(
        Command::new("git")
            .current_dir(source)
            .args(["ls-files", "-z"]),
    );
    for file in files
        .stdout
        .split(|byte| *byte == 0)
        .filter(|file| !file.is_empty())
    {
        let relative = Path::new(std::str::from_utf8(file).expect("UTF-8 tracked path"));
        let destination = stage.join(relative);
        fs::create_dir_all(destination.parent().expect("staged file parent"))
            .expect("create staged directory");
        fs::copy(source.join(relative), destination).expect("copy tracked input");
    }
    let artifacts = scratch.join("artifacts");
    let macros = archive(&stage, &artifacts, "systasis-macros", None);
    let patch = format!(
        "patch.crates-io.systasis-macros.path={}",
        toml_string(&macros)
    );
    let runtime = archive(&stage, &artifacts, "systasis", Some(&patch));

    for (backend, std_enabled) in [("std", true), ("no-std", false)] {
        let consumer = scratch.join(backend);
        fs::create_dir_all(consumer.join("src")).expect("create consumer");
        fs::write(
            consumer.join("Cargo.toml"),
            format!(
                r#"
[package]
name = "packaged-consumer-{backend}"
version = "0.0.0"
edition = "2024"
[workspace]
[features]
default = []
std = ["systasis/std"]
[dependencies]
systasis = {{ path = {}, default-features = false }}
"#,
                toml_string(&runtime)
            ),
        )
        .expect("write isolated consumer manifest");
        fs::write(consumer.join("src/lib.rs"), r#"
#![cfg_attr(not(feature = "std"), no_std)]
#![forbid(unsafe_code)]
pub mod child {
    pub trait IValue {}
    impl IValue for u32 {}
    trait IFresh {}
    impl IFresh for u32 {}
    type Seed = (u32, bool);
    #[systasis::container]
    pub fn run(visit: impl FnOnce(&AppContainer)) {
        #[cfg(all())]
        let (seed, _): Seed = (42, false);
        #[cfg(any())]
        let seed: Missing = missing;
        let Ok(container) = systasis::systasis_container! {
            register_value!(41u32: u32 as IValue);
            register_type_with!(u32 as IFresh, || seed);
        }.build();
        assert_eq!(container.resolve_i_fresh(), 42);
        visit(container);
    }
}
pub mod outer {
    type Alias = super::child::AppContainer;
    struct Stored(u32);
    trait IStored {}
    impl IStored for Stored {}
    trait ICheck {}
    impl ICheck for u32 {}
    mod imported {
        #[allow(non_upper_case_globals)]
        pub const __systasis_children: u8 = 0;
    }
    fn named_scope(scope: &primary::SubContainer<'_>) -> u32 { scope.resolve_i_value() }
    fn named_container(container: &AppContainer<'_>) -> u32 { named_scope(container.primary()) }
    #[systasis::container]
    pub fn run(primary: &Alias) -> u32 {
        let Ok(container) = systasis::systasis_container! {
            register_container!(primary: &Alias);
            register_value!(Stored(resolve_from!(IValue, primary) + 1): Stored as IStored);
            register_type_with!(u32 as ICheck, || {
                use imported::*;
                let _authored = __systasis_children;
                resolve_from!(IFresh, primary)
            });
        }.build();
        assert_eq!(container.resolve_i_check(), 42);
        assert_eq!(named_container(container), 41);
        assert!(core::ptr::eq(container.primary(), container.primary()));
        let guard = container.try_resolve_i_stored_ref().unwrap();
        assert!(matches!(container.try_resolve_i_stored(), Err(systasis::app_container::Error::ValueAccessContention)));
        assert_eq!(guard.0, 42);
        drop(guard);
        let result = container.try_resolve_i_stored().unwrap().0;
        assert!(matches!(container.try_resolve_i_stored(), Err(systasis::app_container::Error::ValueAlreadyConsumed)));
        result
    }
}
pub fn run() -> u32 {
    let mut value = 0;
    child::run(|child| value = outer::run(child));
    value
}
"#).expect("write consumer library");
        fs::write(
            consumer.join("src/main.rs"),
            format!(
                "fn main() {{ assert_eq!(packaged_consumer_{}::run(), 42); }}\n",
                backend.replace('-', "_")
            ),
        )
        .expect("write execution harness");
        let mut run = cargo(&consumer);
        run.args(["run", "--offline", "--config", &patch, "--target-dir"])
            .arg(scratch.join("consumer-target"));
        if std_enabled {
            run.args(["--features", "std"]);
        }
        checked(&mut run);
    }
    for (path, expected) in protected.into_iter().zip(before) {
        assert_eq!(
            fs::read(source.join(path)).expect("read source manifest/lockfile"),
            expected,
            "artifact validation changed source {path}"
        );
    }
    eprintln!(
        "Packaged std/no_std consumers passed in {:.2}s; artifacts: {}",
        started.elapsed().as_secs_f64(),
        scratch.display()
    );
}
