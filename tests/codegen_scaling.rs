//! Opt-in compiler scaling samples; no timing thresholds.
//! Run: `cargo test --test codegen_scaling --offline -- --ignored --nocapture`
//! Repeat with `--no-default-features` for the spin runtime backend.
#![cfg(not(miri))]
#![forbid(unsafe_code)]

#[cfg(all(test, not(miri)))]
mod support;

use std::{
    fmt::Write as _,
    fs,
    path::Path,
    process::{Command, Output},
    time::{Instant, SystemTime, UNIX_EPOCH},
};

const SAMPLES: usize = 3;

fn checked(command: &mut Command) -> Output {
    let output = command.output().expect("execute compiler sample");
    assert!(
        output.status.success(),
        "{command:?}\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn flat(count: usize) -> String {
    let mut source = String::from("#![forbid(unsafe_code)]\n");
    for index in 0..count {
        writeln!(
            source,
            "trait IValue{index} {{}} impl IValue{index} for u64 {{}}"
        )
        .unwrap();
    }
    source.push_str("fn named(container: &AppContainer) -> u64 {\n");
    let calls = (0..count)
        .map(|index| format!("container.resolve_i_value{index}()"))
        .collect::<Vec<_>>()
        .join(" + ");
    writeln!(source, "{calls}\n}}\n#[systasis::container]\nfn main() {{\nlet Ok(container) = systasis::systasis_container! {{").unwrap();
    for index in 0..count {
        writeln!(source, "register_value!({index}u64: u64 as IValue{index});").unwrap();
    }
    writeln!(
        source,
        "}}.build(); assert_eq!(named(container), {}); }}",
        count * (count - 1) / 2
    )
    .unwrap();
    source
}

fn constructors(count: usize) -> String {
    let mut source =
        String::from("#![forbid(unsafe_code)]\ntrait ISeed {} impl ISeed for u64 {}\n");
    for index in 0..count {
        writeln!(
            source,
            "trait IValue{index} {{}} impl IValue{index} for u64 {{}}"
        )
        .unwrap();
    }
    source.push_str("fn named(container: &AppContainer) -> u64 {\n");
    let calls = (0..count)
        .map(|index| format!("container.resolve_i_value{index}()"))
        .collect::<Vec<_>>()
        .join(" + ");
    writeln!(source, "{calls}\n}}\n#[systasis::container]\nfn main() {{").unwrap();
    for index in 0..count {
        writeln!(source, "let offset{index}: u64 = {index};").unwrap();
    }
    source.push_str("let Ok(container) = systasis::systasis_container! {\n");
    source.push_str("register_value!(7u64: u64 as ISeed);\n");
    for index in 0..count {
        writeln!(
            source,
            "register_type_with!(u64 as IValue{index}, move || resolve!(ISeed) + offset{index});"
        )
        .unwrap();
    }
    writeln!(
        source,
        "}}.build(); assert_eq!(named(container), {}); assert_eq!(named(container), {}); }}",
        7 * count + count * (count - 1) / 2,
        7 * count + count * (count - 1) / 2,
    )
    .unwrap();
    source
}

fn nested(depth: usize) -> String {
    let mut source = String::from(
        r#"
#![forbid(unsafe_code)]
mod layer0 {
    pub trait IValue {}
    impl IValue for u64 {}
    fn named(container: &AppContainer) { assert_eq!(container.resolve_i_value(), 17); }
    #[systasis::container]
    pub fn run(visit: impl FnOnce(&AppContainer)) {
        let Ok(container) = systasis::systasis_container! {
            register_value!(17u64: u64 as IValue);
        }.build();
        named(container); visit(container);
    }
}
"#,
    );
    for layer in 1..=depth {
        let previous = layer - 1;
        let lifetimes = (0..layer)
            .map(|index| format!("'a{index}"))
            .collect::<Vec<_>>();
        let child_lifetimes = lifetimes[..previous].join(", ");
        let parameters = lifetimes.join(", ");
        let borrow = &lifetimes[previous];
        let inferred = vec!["'_"; layer].join(", ");
        let child_type = if layer == 1 {
            format!("super::layer{previous}::AppContainer")
        } else {
            format!("super::layer{previous}::AppContainer<{child_lifetimes}>")
        };
        let chain = "child().".repeat(layer);
        // Each layer borrows an independently owned child. Do not equate that
        // short borrow with the child's longer backing lifetimes: child storage
        // can be invariant, so &'a AppContainer<'a> overconstrains nested callers.
        let nested_chain = "child().".repeat(layer - 1);
        writeln!(source, r#"
mod layer{layer} {{
    fn named(container: &AppContainer<{inferred}>) {{ assert_eq!(container.{chain}resolve_i_value(), 17); }}
    fn named_scope(scope: &child::SubContainer<{inferred}>) {{ assert_eq!(scope.{nested_chain}resolve_i_value(), 17); }}
    #[systasis::container]
    pub fn run<{parameters}>(child: &{borrow} {child_type}, visit: impl FnOnce(&AppContainer<{parameters}>)) {{
        let Ok(container) = systasis::systasis_container! {{
            register_container!(child: &{borrow} {child_type});
        }}.build();
        named(container); named_scope(container.child()); visit(container);
    }}
}}
"#).unwrap();
    }
    source.push_str("fn main() { layer0::run(|container0| {\n");
    for layer in 1..=depth {
        writeln!(
            source,
            "layer{layer}::run(container{}, |container{layer}| {{",
            layer - 1
        )
        .unwrap();
    }
    let chain = "child().".repeat(depth);
    writeln!(
        source,
        "assert_eq!(container{depth}.{chain}resolve_i_value(), 17);"
    )
    .unwrap();
    for _ in 0..=depth {
        source.push_str("});\n");
    }
    source.push_str("}\n");
    source
}

#[test]
#[ignore = "independent compiler samples, including codegen/link and execution"]
fn stored_constructor_and_nested_container_scaling() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let backend = if cfg!(feature = "std") {
        "std"
    } else {
        "no-std"
    };
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock after epoch")
        .as_nanos();
    let target = root.join("target/codegen-scaling").join(backend);
    let samples = target.join(format!("samples-{}-{nonce}", std::process::id()));
    fs::create_dir_all(&samples).expect("create sample directory");
    let mut build = Command::new(env!("CARGO"));
    build
        .current_dir(root)
        .args(["build", "--lib", "--offline", "--locked", "--target-dir"])
        .arg(&target);
    if !cfg!(feature = "std") {
        build.arg("--no-default-features");
    }
    let warmed = Instant::now();
    let artifacts = support::Artifacts::build(&mut build);
    eprintln!(
        "Dependency artifact preparation (excluded from sample timing): {:.3}s",
        warmed.elapsed().as_secs_f64()
    );
    let compiler = checked(Command::new("rustc").args(["--version", "--verbose"]));
    eprintln!("{}", String::from_utf8_lossy(&compiler.stdout));
    eprintln!(
        "execution={}/{}, runtime={backend}, edition=2024, rustc opt-level=0, incremental=off, samples={SAMPLES}; dependency library built in dev profile with only backend features",
        std::env::consts::ARCH,
        std::env::consts::OS
    );
    for variable in ["RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS"] {
        eprintln!(
            "dependency build {variable}={:?}; direct rustc sample flags are specified above",
            std::env::var_os(variable)
        );
    }
    eprintln!(
        "Each invocation processes a distinct crate, not a cached Cargo check. Check includes expansion/typechecking+metadata; build includes expansion/typechecking+codegen+link. Execution follows outside timing. Source bytes and executable bytes are measured, not expanded-token size."
    );

    let cases = [
        ("flat1", 1, 0, flat(1)),
        ("flat8", 8, 0, flat(8)),
        ("flat32", 32, 0, flat(32)),
        ("constructors1", 2, 0, constructors(1)),
        ("constructors8", 9, 0, constructors(8)),
        ("constructors32", 33, 0, constructors(32)),
        ("nested1", 1, 1, nested(1)),
        ("nested2", 1, 2, nested(2)),
        ("nested3", 1, 3, nested(3)),
    ];
    for round in 0..SAMPLES {
        // Reverse case ordering every other round to reduce consistent order bias.
        for position in 0..cases.len() {
            let index = if round % 2 == 0 {
                position
            } else {
                cases.len() - position - 1
            };
            let (name, registrations, depth, source) = &cases[index];
            let crate_name = format!("{name}_{round}");
            let file = samples.join(format!("{crate_name}.rs"));
            fs::write(&file, source).expect("write independent source fixture");
            let compile = |emit: &str| {
                let mut command = artifacts.rustc();
                command
                    .args([
                        "--edition=2024",
                        "--crate-name",
                        &crate_name,
                        "--emit",
                        emit,
                        "-C",
                        "opt-level=0",
                    ])
                    .arg(&file)
                    .arg("--out-dir")
                    .arg(&samples);
                let start = Instant::now();
                checked(&mut command);
                start.elapsed().as_secs_f64()
            };
            let check_seconds = compile("metadata");
            let build_seconds = compile("link");
            let executable = samples.join(&crate_name);
            checked(&mut Command::new(&executable));
            eprintln!(
                "{name} sample={} registrations={registrations} child_depth={depth} source_bytes={} executable_bytes={} check_seconds={check_seconds:.6} build_link_seconds={build_seconds:.6}",
                round + 1,
                source.len(),
                fs::metadata(&executable).expect("measure executable").len()
            );
        }
    }
    eprintln!(
        "Artifacts: {}. No timing thresholds, expanded-code-size measurements, or universal scaling claim.",
        samples.display()
    );
}
