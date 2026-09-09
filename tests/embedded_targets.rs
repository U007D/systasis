//! Compile/link checks only; these do not execute firmware on physical hardware.
#![cfg(all(not(miri), feature = "experimental-hardware"))]
#![forbid(unsafe_code)]

use std::{fs, path::PathBuf, process::Command};

#[test]
fn generated_no_std_containers_link_on_native_cas_targets_and_require_platform_fallback() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    for (target_triple, links) in [
        ("thumbv8m.main-none-eabihf", true),
        ("riscv32imac-unknown-none-elf", true),
        ("thumbv6m-none-eabi", false),
    ] {
        let target_dir = root.join("target/embedded-contracts").join(target_triple);
        let output = Command::new(env!("CARGO"))
            .current_dir(&root)
            .args([
                "build",
                "--lib",
                "--offline",
                "--locked",
                "--no-default-features",
                "--features",
                "portable-atomic",
                "--target",
                target_triple,
                "--target-dir",
            ])
            .arg(&target_dir)
            .output()
            .expect("build embedded runtime");
        assert!(
            output.status.success(),
            "{target_triple} library build: {}",
            String::from_utf8_lossy(&output.stderr)
        );

        let source = target_dir.join("generated_firmware.rs");
        fs::write(
            &source,
            r#"
#![no_std]
#![no_main]
struct Value(u32);
trait IValue {}
impl IValue for Value {}
#[panic_handler]
fn panic(_: &core::panic::PanicInfo<'_>) -> ! { loop { core::hint::spin_loop(); } }
#[systasis::container]
#[unsafe(no_mangle)]
pub extern "C" fn systasis_embedded_probe_entry() -> ! {
    let Ok(container) = systasis::systasis_container! {
        register_value!(Value(41): Value as IValue);
    }.build();
    {
        let mut guard = container.try_resolve_i_value_ref_mut().unwrap();
        guard.0 += 1;
        core::hint::black_box(guard.0);
    }
    core::hint::black_box(container.try_resolve_i_value().unwrap().0);
    loop { core::hint::spin_loop(); }
}
"#,
        )
        .expect("write generated firmware fixture");
        let artifact = target_dir.join("generated_firmware.elf");
        let library_dir = target_dir.join(target_triple).join("debug");
        let output = Command::new("rustc")
            .args([
                "--edition=2024",
                "--target",
                target_triple,
                "-C",
                "panic=abort",
                "-C",
                "link-arg=-esystasis_embedded_probe_entry",
            ])
            .arg(&source)
            .arg("-o")
            .arg(&artifact)
            .arg("--extern")
            .arg(format!(
                "systasis={}",
                library_dir.join("libsystasis.rlib").display()
            ))
            .arg("-L")
            .arg(format!("dependency={}", library_dir.join("deps").display()))
            .arg("-L")
            .arg(format!(
                "dependency={}",
                target_dir.join("debug/deps").display()
            ))
            .output()
            .expect("link generated firmware fixture");
        fs::write(target_dir.join("link-diagnostics.txt"), &output.stderr)
            .expect("retain link diagnostics");
        let diagnostics = String::from_utf8_lossy(&output.stderr);
        if links {
            assert!(
                output.status.success(),
                "{target_triple} link: {diagnostics}"
            );
            let bytes = fs::read(&artifact).expect("read linked ELF");
            assert_eq!(
                &bytes[..4],
                b"\x7fELF",
                "{target_triple} did not produce ELF"
            );
        } else {
            assert!(
                !output.status.success(),
                "fallback target unexpectedly linked without a critical-section implementation"
            );
            assert!(
                diagnostics.contains("undefined symbol: _critical_section_1_0_acquire"),
                "{target_triple}: {diagnostics}"
            );
            assert!(
                diagnostics.contains("undefined symbol: _critical_section_1_0_release"),
                "{target_triple}: {diagnostics}"
            );
        }
    }
}
