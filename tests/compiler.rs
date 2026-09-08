//! Rust-driven downstream checks with diagnostic-reason validation.
#![cfg(not(miri))]
#![forbid(unsafe_code)]

use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::{Mutex, OnceLock},
};

static COMPILER: Mutex<()> = Mutex::new(());
static DRIVER: OnceLock<Driver> = OnceLock::new();

struct Driver {
    manifest: PathBuf,
    target: PathBuf,
}

impl Driver {
    fn initialize() -> Self {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let backend = if cfg!(feature = "std") {
            "std"
        } else {
            "no-std"
        };
        let driver = Self {
            manifest: root.join("tests/fixtures/contracts/Cargo.toml"),
            target: root.join("target/compiler").join(backend),
        };
        fs::create_dir_all(&driver.target).expect("create diagnostic directory");
        let baseline = driver.check(None);
        driver.record("baseline", &baseline);
        assert!(
            baseline.status.success(),
            "fixture baseline must compile: {}",
            String::from_utf8_lossy(&baseline.stderr)
        );
        driver
    }

    fn check(&self, case: Option<&str>) -> Output {
        let mut command = Command::new(env!("CARGO"));
        command
            .args([
                "check",
                "--lib",
                "--offline",
                "--locked",
                "--no-default-features",
                "--message-format=short",
                "--color=never",
                "--manifest-path",
            ])
            .arg(&self.manifest)
            .arg("--target-dir")
            .arg(&self.target)
            .env_remove("RUSTC_BOOTSTRAP")
            .env("CARGO_BUILD_RUSTC_WRAPPER", "")
            .env("CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER", "");
        let features: Vec<_> = cfg!(feature = "std")
            .then_some("std")
            .into_iter()
            .chain(case)
            .collect();
        if !features.is_empty() {
            command.args(["--features", &features.join(",")]);
        }
        command.output().expect("run fixture compiler")
    }

    fn record(&self, case: &str, output: &Output) {
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        fs::write(self.target.join(format!("{case}.log")), text).expect("retain diagnostics");
    }
}

fn has_error(diagnostics: &str, code: &str, fragments: &[&str]) -> bool {
    diagnostics.lines().any(|line| {
        line.contains(&format!("error[{code}]"))
            && fragments.iter().all(|fragment| line.contains(fragment))
    })
}

fn rejects(case: &str, code: &str, fragments: &[&str]) {
    let _serial = COMPILER.lock().expect("compiler driver lock");
    let driver = DRIVER.get_or_init(Driver::initialize);
    let output = driver.check(Some(case));
    driver.record(case, &output);
    let diagnostics = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success(),
        "invalid fixture unexpectedly compiled"
    );
    assert!(
        has_error(&diagnostics, code, fragments),
        "wrong rejection for {case}:\n{diagnostics}"
    );
}

#[test]
fn matcher_rejects_warning_and_unrelated_error_fragments() {
    assert!(!has_error(
        "warning: Copy\nerror[E0277]: Send",
        "E0277",
        &["Copy"]
    ));
    assert!(!has_error("error[E0308]: Copy", "E0277", &["Copy"]));
    assert!(has_error(
        "src/lib.rs:9:3: error[E0277]: Error: Copy",
        "E0277",
        &["Error", "Copy"]
    ));
}

#[test]
fn public_error_is_not_copy() {
    rejects("bad-copy-error", "E0277", &["Copy"]);
}

#[test]
fn ordinary_values_do_not_implement_fallible() {
    rejects("bad-fallible", "E0277", &["u32", "Fallible"]);
}

#[cfg(not(feature = "std"))]
#[test]
fn poison_variant_does_not_exist_without_std() {
    rejects("bad-poison-in-no-std", "E0599", &["PoisonedLock"]);
}

#[cfg(feature = "std")]
#[test]
fn std_guard_is_not_send() {
    rejects("bad-send-guard", "E0277", &["RwLockReadGuard", "sent"]);
}

#[cfg(feature = "std")]
#[test]
fn std_mut_guard_is_not_send() {
    rejects("bad-send-mut-guard", "E0277", &["RwLockWriteGuard", "sent"]);
}

#[test]
fn guard_cannot_escape_backing_slot() {
    rejects("bad-guard-escape", "E0515", &["slot"]);
}

#[test]
fn guard_does_not_make_cell_sync() {
    rejects("bad-sync-cell-guard", "E0277", &["Cell", "shared"]);
}
