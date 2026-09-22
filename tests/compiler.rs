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
        // Short diagnostics begin with an optional path:line:column location.
        // Inspect severity before searching message text: a warning can quote
        // an error code without being evidence of that compiler rejection.
        let diagnostic = line
            .split_once(": ")
            .filter(|(location, _)| {
                let mut fields = location.rsplitn(3, ':');
                fields
                    .next()
                    .is_some_and(|column| column.parse::<usize>().is_ok())
                    && fields
                        .next()
                        .is_some_and(|row| row.parse::<usize>().is_ok())
                    && fields.next().is_some()
            })
            .map_or(line, |(_, diagnostic)| diagnostic);
        diagnostic
            .strip_prefix(&format!("error[{code}]:"))
            .is_some_and(|message| fragments.iter().all(|fragment| message.contains(fragment)))
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
        "src/lib.rs:9:3: warning: quoted error[E0277]: Copy\nsrc/lib.rs:10:3: error[E0308]: unrelated mismatch",
        "E0277",
        &["Copy"]
    ));
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
fn public_error_supports_value_traits() {
    let _serial = COMPILER.lock().expect("compiler driver lock");
    let driver = DRIVER.get_or_init(Driver::initialize);
    let output = driver.check(Some("error-value-traits"));
    driver.record("error-value-traits", &output);
    assert!(
        output.status.success(),
        "resolution errors must implement their required value traits:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn ordinary_values_do_not_implement_fallible() {
    rejects("bad-fallible", "E0277", &["u32", "Fallible"]);
}

#[test]
fn poison_variant_does_not_exist() {
    rejects("bad-poison-in-no-std", "E0599", &["PoisonedLock"]);
}

#[test]
fn guard_cannot_escape_backing_slot() {
    rejects("bad-guard-escape", "E0515", &["slot"]);
}

#[test]
fn guard_does_not_make_cell_sync() {
    rejects("bad-sync-cell-guard", "E0277", &["Cell", "shared"]);
}

#[test]
fn reserved_reference_cannot_escape_backing_slot() {
    rejects("bad-reserved-escape", "E0515", &["slot"]);
}

#[test]
fn reserved_reference_prevents_moving_backing_slot() {
    rejects("bad-move-reserved-slot", "E0505", &["slot", "borrowed"]);
}

#[test]
fn reservation_does_not_make_cell_slot_sync() {
    rejects("bad-sync-cell-slot", "E0277", &["Cell", "shared"]);
}

#[test]
fn reservation_does_not_make_rc_slot_send() {
    rejects("bad-send-rc-slot", "E0277", &["Rc", "sent"]);
}

#[test]
fn shared_guard_requires_sync_payload_to_be_send() {
    rejects("bad-send-cell-reader", "E0277", &["Cell", "shared"]);
}

#[test]
fn mutable_guard_preserves_payload_lifetime_invariance() {
    rejects("bad-mutable-lifetime", "E0521", &["escapes"]);
}

#[test]
fn local_slot_is_not_sync() {
    rejects("bad-local-sync", "E0277", &["RefCell", "shared"]);
}

#[test]
fn local_guard_is_not_send() {
    rejects("bad-local-guard-send", "E0277", &["sent"]);
}

#[test]
fn readonly_slot_has_no_take_operation() {
    rejects("bad-readonly-take", "E0599", &["try_resolve"]);
}
