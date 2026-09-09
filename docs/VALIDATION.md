# Implementation validation

## Current generated-container checks

Local namespaces (58bc418/c577507) pass full workspace std/no_std tests and
both Clippy configurations. Coverage includes seven public behavior tests,
two parser tests and ten compiler cases. Child composition is not yet implemented.
Ordinary explicit constructor imports (ac9dc3b) pass seventeen capture unit
tests and eleven constructor tests on both backends; function-local module
imports remain rejected because those modules are not hoisted.

Combined dyn groups (df66c32) pass seven focused tests on both backends, including
associated bindings, generic targets, independent lifetimes and returned guards.
A further scoped-thread test verifies Send read guards for explicitly Sync dyn
targets and successful consumption after guard release. Source-relative path
rebasing (0f568ed/11c854e) passes module-collision and local-import regressions.
The combined implementation passed full workspace std/no_std suites and Clippy;
the last added returned-guard/thread/import cases also passed focused checks.

Generic integration commits 99b9d7f/e2b639a preserve declaration-site Copy policy
and authored alias parameters for stored values, fresh constructors, typed
captures, returned guards and single-trait dyn access. Dedicated behavior,
negative diagnostics and downstream provider/caller checks pass both backends.
Full workspace tests and both Clippy configurations passed during integration;
the subsequently added cross-crate suite also passed both backends.
Runtime and generated-fixture `cargo check` pass for ARM
`thumbv8m.main-none-eabihf` and RISC-V `riscv32imac-unknown-none-elf`, including a
generic generated container. This is compile evidence, not linking or board tests.

Static group registration (90ff0d7) passes seven public-API tests on each backend,
with whole-group/member distinction and override diagnostics. Additional tests
exercise all six permutations of three traits and qualified names with an
associated-type binding. Generic-policy evidence helpers (6aac5bb) pass four
policy tests per backend and an intended indirect-Copy diagnostic. Generic
container generation is being integrated; helper tests alone do not establish it.

Capture follow-ups b7e99c6 through f7059c0 add structural tuple/array type
extraction, explicit reference patterns, and absolute local-import preservation.
Eleven custom-constructor tests pass on each backend. Full workspace tests and
both Clippy runs passed with the import integration; the final reference-pattern
addition passed focused constructor suites and macro tests. Commit 8719086 adds
three dyn type-query tests and two compiler cases (29 cases total), including
associated-type bindings. These changes introduce no unsafe code or dependencies.

Latest update: typed captures and repeatable custom constructors pass both full
stable workspace suites and both Clippy configurations. Seven constructor tests
cover repeated calls, exact fallible returns, capture cleanup, and returned
references/guards. The compiler driver now checks 25 downstream cases, including
capture ownership, missing annotations, incompatible Send state, and constructor
borrowing excluding owned resolution. Build inference now has nine tests,
including `.build()?` success/failure and chained Result methods.
These safe-only changes add neither unsafe code nor dependencies; Miri was not
rerun. The Miri counts below describe the earlier revision, not these additions.

Follow-up dyn query wiring passes the focused `dyn_container` and
`container_compiler` suites on both backends, plus both full-target Clippy runs.
There are now five dyn tests and 27 downstream compiler cases. Queries work
during initialization and repeated construction; negative cases check missing
dyn opt-in and preservation of constructor-borrow ownership exclusions.

Production commits bff03b7 through 678d63d add stored-value generation, owned
dependencies, Default constructors, overrides, checked access and cloning,
single-trait dyn access, build inference, and import/initializer hygiene.
Both full stable workspace suites pass (std and no_std runtime on the host),
as do both Clippy configurations with warnings denied. The owned example runs
with warnings denied and the documented macro import.

The six integration suites `owned_container`, `container_access`,
`container_hygiene`, `dyn_container`, `fresh_container`, and `build_inference`
also pass Miri: 24 tests per backend. This verifies the newly integrated
generated/runtime paths; it does not revalidate hardware or prove soundness.
The generator adds no unsafe code or runtime allocation mechanism.

`container_compiler` checks 16 downstream cases against explicit diagnostics.
The scheduler tests all 65,536 four-node directed graphs, checking dependency
order or the validity of each reported cycle. Its separate ordering test checks
frozen layers. The existing storage/compiler suites remain enabled.

Run the six named integration suites with `cargo +nightly miri test --test NAME`,
and repeat with `--no-default-features`. Native suites use the workspace commands
below. See [README.md](../README.md) for remaining implementation work.

## Historical runtime evidence

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
