# Runtime foundation validation

Current update: std parking_lot 0.12.5/send_guard and no_std spin each pass
51 native tests and 31 Miri tests (storage, reservation, unsynchronized).
Both Clippy configurations pass with warnings denied. No poisoning remains.
ReadSlot/LocalTakeSlot are runtime primitives; generator policy selection is
pending. See SAFETY.md for current behavior. The table and notes below are
historical evidence predating this backend change; embedded and Tree Borrows
checks have not been rerun for the new dependency.

Verified 2026-09-08, aarch64 macOS, stable rustc 1.98.1.
This is partial runtime implementation, not a working generated container.

| Configuration | Passing checks |
| --- | --- |
| std | 49 tests plus 3 compile-fail doctests |
| no_std runtime on host | 42 tests |
| Miri std, default and Tree Borrows | 18 storage tests and 10 reservation tests |
| Miri no_std runtime, default and Tree Borrows | 16 storage tests and 9 reservation tests |
| Embedded compilation | thumbv8m.main-none-eabihf and riscv32imac-unknown-none-elf |

Both runtime configurations pass Clippy with warnings denied and formatting.
The Miri installation reports `miri 0.1.0 (4b0c9d76ae 2026-05-10)`.
No extra feature gate or RUSTC_BOOTSTRAP is used by the runtime tests.
Compiler fixtures run under a native Rust test driver, not under Miri.
The five std error-conversion Miri tests also passed in the 2026-09-07 foundation
run; that source is unchanged. Each current native backend includes twelve
Rust-driver tests, covering intended errors and diagnostic-matcher checks.

## Reproduction

```sh
cargo +stable test --workspace --offline --locked
cargo +stable test --workspace --no-default-features --offline --locked
cargo +stable clippy --workspace --all-targets --offline --locked -- -D warnings
cargo +stable clippy --workspace --all-targets --no-default-features --offline --locked -- -D warnings
cargo +stable fmt --all -- --check
cargo +nightly miri test -p systasis --lib --offline --locked
cargo +nightly miri test -p systasis --test storage --offline --locked
cargo +nightly miri test -p systasis --test storage --no-default-features --offline --locked
cargo +nightly miri test -p systasis --test reservation --offline --locked
cargo +nightly miri test -p systasis --test reservation --no-default-features --offline --locked
cargo +stable check --no-default-features --target thumbv8m.main-none-eabihf --offline --locked
cargo +stable check --no-default-features --target riscv32imac-unknown-none-elf --offline --locked
```

Offline commands require cached dependencies. Select a compatible installed
Miri toolchain; the local `+nightly` alias identifies the version recorded above,
not a pinned globally reproducible toolchain name.
Repeat the storage and reservation Miri commands with
`MIRIFLAGS=-Zmiri-tree-borrows` for the recorded second aliasing model.
An ancestor Cargo configuration in the development environment wraps rustc
with Clippy. Runs here set `CARGO_BUILD_RUSTC_WRAPPER=`; Clippy additionally uses
`RUSTC_WRAPPER=`. No global configuration was changed.

## Boundaries

- Permanent shared reservations now use the accepted lock-protected flag and
  split payload. Std has seven unsafe blocks and three unsafe Sync impls;
  Spin has four unsafe blocks and one unsafe Sync impl. See SAFETY.md for each
  obligation. No lifetime extension, new dependency or allocation was added.
- Reservation cases check permanent contention after last reference use and
  dependent destruction, compatible reads/cloning/projection, repeated and
  failed acquisition, consumption, poisoning, concurrent access, forgotten
  guards, non-Sync local values, alignment and zero-sized payloads.
- Compiler cases reject escaping reserved references, moving their borrowed
  slot, incorrect Send/Sync bounds and mutable payload lifetime substitution.
  Positive controls preserve Spin guard Send bounds and slot Send-without-Sync.
- Poison conversion drops the acquired guard before constructing PoisonError<()>.
  Reviewed std source shows unwind-built new is plain construction; abort-built
  PoisonError contains an uninhabited field and cannot originate from a lock.
- thiserror has default features disabled; std explicitly forwards thiserror/std.
  Feature-tree inspection confirms no runtime thiserror/std activation in no_std.
  Its proc-macro dependencies build for the host.
- Cross-compilation is not firmware linking or physical-board testing. Neither
  portable-atomic integration nor the temporary hardware feature exists yet.
- Container generation, scoped composition, scheduling, unchecked access,
  full allocation/performance validation and packaged-consumer testing remain.
