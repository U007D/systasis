//! Run with stable rustc; no dependency on the production nightly macro crate.
#![forbid(unsafe_code, unstable_features)]

use std::{env, fs, path::PathBuf, process::Command};

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

fn main() {
    let root = PathBuf::from(env!("PWD")).join("research/stable-replacements");
    let output_dir = env::temp_dir().join(format!(
        "systasis-stable-replacements-{}",
        std::process::id()
    ));
    fs::create_dir_all(&output_dir).unwrap();
    let version = Command::new("rustc")
        .args(["+stable", "--version"])
        .output()
        .unwrap();
    fs::write(output_dir.join("compiler.txt"), &version.stdout).unwrap();
    print!("{}", String::from_utf8_lossy(&version.stdout));

    let provider = rustc()
        .args(["--crate-type=rlib", "--crate-name=stable_metadata"])
        .arg(root.join("provider.rs"))
        .arg("--out-dir")
        .arg(&output_dir)
        .output()
        .unwrap();
    fs::write(output_dir.join("provider.stderr"), &provider.stderr).unwrap();
    assert!(
        provider.status.success(),
        "{}",
        String::from_utf8_lossy(&provider.stderr)
    );
    let executable = output_dir.join("consumer");
    let consumer = rustc()
        .arg(root.join("consumer.rs"))
        .arg("--extern")
        .arg(format!(
            "stable_metadata={}",
            output_dir.join("libstable_metadata.rlib").display()
        ))
        .arg("-o")
        .arg(&executable)
        .output()
        .unwrap();
    fs::write(output_dir.join("consumer.stderr"), &consumer.stderr).unwrap();
    assert!(
        consumer.status.success(),
        "{}",
        String::from_utf8_lossy(&consumer.stderr)
    );
    assert!(Command::new(&executable).status().unwrap().success());
    println!("PASS metadata provider (no_std + alloc) and downstream consumer");

    for (name, expression) in [
        (
            "local_container_not_send",
            "stable_metadata::LocalContainer::new()",
        ),
        (
            "local_error_not_send",
            "stable_metadata::local_failure().unwrap_err()",
        ),
    ] {
        let input = output_dir.join(format!("{name}.rs"));
        fs::write(
            &input,
            format!("fn send<T: Send>(_: T) {{}} fn main() {{ send({expression}); }}"),
        )
        .unwrap();
        let output = rustc()
            .arg(&input)
            .arg("--extern")
            .arg(format!(
                "stable_metadata={}",
                output_dir.join("libstable_metadata.rlib").display()
            ))
            .arg("--out-dir")
            .arg(&output_dir)
            .output()
            .unwrap();
        let diagnostic = String::from_utf8_lossy(&output.stderr);
        fs::write(output_dir.join(format!("{name}.stderr")), &output.stderr).unwrap();
        assert!(
            !output.status.success()
                && diagnostic
                    .lines()
                    .any(|line| line.starts_with("error[E0277]"))
                && diagnostic.contains("cannot be sent between threads safely"),
            "{name}: {diagnostic}"
        );
        println!("PASS {name}: rejected with E0277");
    }

    let cases = [
        (
            "opaque_factory_metadata",
            r#"
#![allow(dead_code)]
trait Config { fn text(&self) -> &str; }
impl Config for String { fn text(&self) -> &str { self } }
fn factory() -> impl Config { String::from("opaque") }
mod factory { pub type Output = impl super::Config; }
fn main() {}
"#,
            Some("E0658"),
        ),
        (
            "unknown_error_metadata",
            r#"
#![allow(dead_code)]
fn factory() -> Result<u8, impl core::error::Error> { "bad".parse() }
mod factory { pub type Error = _; }
fn main() {}
"#,
            Some("E0121"),
        ),
        (
            "fallibility_not_question_count",
            r#"
trait Output { type Value; }
impl<T, F: FnOnce() -> T> Output for F { type Value = T; }
type Never = <fn() -> ! as Output>::Value;
fn source() -> Result<u8, Never> { Ok(42) }
fn build() -> Result<u8, Never> { Ok(source()?) }
fn main() { let Ok(value) = build(); assert_eq!(value, 42); }
"#,
            None,
        ),
        (
            "opaque_capture_reconstruction",
            r#"
#![allow(dead_code)]
trait Config { fn number(&self) -> u8; }
impl Config for u8 { fn number(&self) -> u8 { *self } }
fn factory() -> impl Config { 42u8 }
struct Container { config: u8 }
fn main() { let config = factory(); let _ = Container { config }; }
"#,
            Some("E0308"),
        ),
        (
            "opaque_capture_generic_control",
            r#"
trait Config { fn number(&self) -> u8; }
impl Config for u8 { fn number(&self) -> u8 { *self } }
fn factory() -> impl Config { 42u8 }
struct Container<C> { config: C }
impl<C: Config> Container<C> { fn resolve(&self) -> u8 { self.config.number() } }
fn receive<C: Config>(container: &Container<C>) -> u8 { container.resolve() }
fn main() { let config = factory(); let c = Container { config }; assert_eq!(receive(&c), 42); }
"#,
            None,
        ),
    ];
    for (name, source, error_code) in cases {
        let input = output_dir.join(format!("{name}.rs"));
        let executable = output_dir.join(name);
        fs::write(&input, source).unwrap();
        let output = rustc()
            .arg(&input)
            .arg("-o")
            .arg(&executable)
            .output()
            .unwrap();
        let diagnostic = String::from_utf8_lossy(&output.stderr);
        fs::write(output_dir.join(format!("{name}.stderr")), &output.stderr).unwrap();
        if let Some(code) = error_code {
            let error_line = format!("error[{code}]");
            assert!(
                !output.status.success()
                    && diagnostic.lines().any(|line| line.starts_with(&error_line)),
                "{name}: {diagnostic}"
            );
            println!("PASS {name}: rejected with {code}");
        } else {
            assert!(output.status.success(), "{name}: {diagnostic}");
            assert!(Command::new(&executable).status().unwrap().success());
            println!("PASS {name}: compiled and ran");
        }
    }
    println!("Artifacts: {}", output_dir.display());
}
