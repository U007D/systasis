//! Reproduce the candidate using stable Rust with no compiler bypasses.
#![forbid(unsafe_code, unstable_features)]

use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

fn rustc() -> Command {
    let mut command = Command::new("rustc");
    command.args([
        "+stable",
        "--edition=2024",
        "-Dwarnings",
        "-Funstable-features",
    ]);
    command.env_remove("RUSTC_BOOTSTRAP");
    command
}

fn record(output: Output, path: &Path) -> Output {
    fs::write(path, &output.stderr).unwrap();
    output
}

fn main() {
    let source = PathBuf::from(env!("PWD")).join(file!());
    let root = source.parent().unwrap();
    let results = std::env::args_os().nth(1).map_or_else(
        || std::env::temp_dir().join(format!("systasis-stable-rpit-{}", std::process::id())),
        PathBuf::from,
    );
    fs::create_dir_all(&results).unwrap();
    let version = Command::new("rustc")
        .args(["+stable", "--version"])
        .output()
        .unwrap();
    print!("{}", String::from_utf8_lossy(&version.stdout));
    fs::write(results.join("compiler.txt"), &version.stdout).unwrap();

    for (mode, local) in [("core", false), ("alloc", true)] {
        let output_dir = results.join(mode);
        fs::create_dir_all(&output_dir).unwrap();
        let mut compiler = rustc();
        compiler
            .args(["--crate-type=rlib", "--crate-name=stable_rpit"])
            .arg(root.join("provider.rs"))
            .arg("--out-dir")
            .arg(&output_dir);
        if local {
            compiler.args(["--cfg", "feature=\"local_probes\""]);
        }
        let output = record(
            compiler.output().unwrap(),
            &output_dir.join("provider.stderr"),
        );
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );

        let dependency = format!(
            "stable_rpit={}",
            output_dir.join("libstable_rpit.rlib").display()
        );
        let mut compiler = rustc();
        compiler
            .arg(root.join("consumer.rs"))
            .arg("--extern")
            .arg(&dependency)
            .arg("-o")
            .arg(output_dir.join("consumer"));
        if local {
            compiler.args(["--cfg", "feature=\"local_probes\""]);
        }
        let output = record(
            compiler.output().unwrap(),
            &output_dir.join("consumer.stderr"),
        );
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let output = Command::new(output_dir.join("consumer")).output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        fs::write(output_dir.join("consumer.stdout"), &output.stdout).unwrap();
        println!("PASS {mode}: no_std provider and downstream runtime assertions");

        if !local {
            continue;
        }
        let cases = [
            (
                "local_capture_not_send",
                "fn send<T: Send>(_: T) {} fn main() { let Ok(c) = stable_rpit::local_capture::SystasisContainer::build(); send(c); }",
                "E0277",
                "cannot be sent",
            ),
            (
                "local_error_not_send",
                "fn send<T: Send>(_: T) {} fn main() { if let Err(e) = stable_rpit::local_failure::SystasisContainer::build() { send(e); } }",
                "E0277",
                "cannot be sent",
            ),
            (
                "fallible_pattern_rejected",
                "fn main() { let Ok(_) = stable_rpit::mixed_success::SystasisContainer::build(); }",
                "E0005",
                "refutable pattern",
            ),
            (
                "bare_container_changes_api",
                "fn receive(_: &stable_rpit::infallible::SystasisContainer) {} fn main() { let Ok(c) = stable_rpit::infallible::SystasisContainer::build(); receive(&c); }",
                "E0308",
                "mismatched types",
            ),
            (
                "bare_error_changes_api",
                "fn consume(_: stable_rpit::parse_failure::SystasisContainerError) {} fn main() {}",
                "E0107",
                "missing generics",
            ),
            (
                "borrow_cannot_escape",
                "fn main() { let c; { let value = 42; c = stable_rpit::borrowed(&value); } let _ = c.resolve_i_number(); }",
                "E0597",
                "does not live long enough",
            ),
        ];
        for (name, source, error, reason) in cases {
            let path = output_dir.join(format!("{name}.rs"));
            fs::write(&path, source).unwrap();
            let output = record(
                rustc()
                    .arg(&path)
                    .arg("--extern")
                    .arg(&dependency)
                    .arg("--out-dir")
                    .arg(&output_dir)
                    .output()
                    .unwrap(),
                &output_dir.join(format!("{name}.stderr")),
            );
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert!(
                !output.status.success()
                    && stderr
                        .lines()
                        .any(|line| line.starts_with(&format!("error[{error}]")))
                    && stderr.contains(reason),
                "{name}: {stderr}"
            );
            println!("PASS {name}: expected {error}");
        }
    }
    println!("Evidence: {}", results.display());
}
