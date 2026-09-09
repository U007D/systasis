# Implementation validation

## Current generated-container checks

### Allocation observations

`tests/allocations.rs` has thirteen passing checks on std and no_std runtime
backends, including Miri. Eight workload tests observe no alloc, alloc_zeroed
or realloc calls during construction, checked resolution, explicit cloning,
returned guards, nested scopes, failed-build cleanup and owner destruction.
Copy cloning has an observable nonallocating Clone implementation; read-only
storage coverage exercises the runtime primitive, not a generated registration.
Five instrumentation controls check all allocator entry points, an empty window,
thread isolation, unwind cleanup and rejection of nested measurement windows.

The user approved the test-only System-forwarding allocator on 2026-09-09.
Its unsafe operations and review are described in [SAFETY.md](SAFETY.md).
Counts are per-thread allocator calls within the measured closure, not bytes or
an optimizer-independent proof. Values returned from the closure drop outside
the window; lifecycle fixtures enclose the hidden container owner's whole scope
so its destruction is measured. The no-allocation assertion deliberately permits
deallocation of preexisting caller state. It does not assert zero allocator events.
Host no_std tests still use std for the test harness and counting allocator;
they are not an allocator installation on embedded hardware.

Reproduce on stable with `cargo +stable test --offline --locked --test allocations`,
then add `--release`, `--no-default-features`, or both. Miri uses
`cargo +nightly miri test --offline --locked --test allocations`, repeated with
`--no-default-features`. Debug/release positive controls detect if a changed
optimizer or standard library stops exercising an expected allocator hook.

### Other integration evidence

Portable atomics (3a55999) use optional portable-atomic 1.15.0 with its
critical-section feature, without enabling either unsafe platform assumption.
Feature-tree inspection for thumbv6m confirms no runtime std feature. The
`storage`, `reservation`, `unsynchronized` and `child_unchecked` Miri suites pass
with portable atomics enabled on both host backends: 32 tests per backend.
Those runs exercise native host atomics, not a physical target's critical section.

`tests/generic_cross_crate.rs` compiles and executes downstream composition through
generic aliases and import renames, including non-static child references and
nameable child scopes, on std/no_std. Direct `resolve_type_from!` registered-type
projections now preserve child declaration-site Copy policy for generic arrays
and borrowed values; the previous negative gap fixture is now positive coverage.
`tests/child_copy_policy.rs` verifies that unbounded generic children instantiated
with `u32` remain consumable, receiving `!Sync` parents use local borrow guards,
Copy projections remain plain, and explicit parent `T: Copy` registration bounds
take precedence when the parent registers the ordinary type `T`.

Child composition (0f23293/cda2e83) supports named shared descriptors, aliases,
nested accessors and `_from` expression/type queries through child namespaces.
Tests cover inherited ownership exclusions, direct and transitive consuming
factories, independent siblings and non-static child lifetimes. The full std
all-feature and no_std unchecked-enabled suites pass; both Clippy configurations
pass. The subsequently added `child_unchecked` test passes natively and under
Miri on both backends, checking guard retention, contention and consumption.
No new dependency or unsafe storage mechanism was introduced. Cross-crate child
composition and semantic equivalence of differently spelled interface keys need
further validation; do not infer them from the module-level alias tests.

Reproduce the new Miri check with `cargo +nightly miri test --offline
--features resolve_unchecked --test child_unchecked`, adding
`--no-default-features` for spin. Compiler-negative tests run natively.

The following entries record earlier revisions rather than cumulative counts.

Unchecked generation (694a7a4) passes four behavior tests natively and under Miri
on both backends. Feature-enabled compiler cases verify unsafe context, Copy/fresh
method absence and constructor-borrow ownership exclusion. Both feature-enabled
Clippy configurations pass. The full no_std feature-enabled workspace suite
passes, including the forbid-unsafe consumer regression; the full std run passed
before that final regression, which also passes its focused std check.

Local namespaces (58bc418/c577507) pass full workspace std/no_std tests and
both Clippy configurations. Coverage includes seven public behavior tests,
two parser tests and ten compiler cases. That revision did not implement child composition.
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

## Current boundaries

Local package validation (378d2f2) found and fixed missing license texts in the
proc-macro archive. `cargo +stable package --offline -p systasis-macros` now
includes both texts and verifies the extracted package. Runtime packaging and
an extracted no_std build pass with a command-line crates.io patch pointing at
that extracted macro package. This is local artifact validation, not evidence
that the unpublished dependency can be downloaded from a registry. No package
was published; no persistent patch or lockfile change was retained.

- The legacy reservation API retains its regression tests but is not called by
  current generated containers. Stored services retaining internal borrows remain
  deferred. See SAFETY.md for the current split-payload storage obligations.
- Reservation cases cover contention, compatible reads/cloning/projection,
  repeated and failed acquisition, consumption, concurrent access, forgotten
  guards, non-Sync local values, alignment and zero-sized payloads.
- Compiler cases reject escaping reserved references, moving their borrowed
  slot, incorrect Send/Sync bounds and mutable payload lifetime substitution.
  Positive controls preserve Spin guard Send bounds and slot Send-without-Sync.
- Both current backends are nonpoisoning; the old poison-conversion investigation
  above is historical, not a current error case or implementation obligation.
- thiserror has default features disabled; std explicitly forwards thiserror/std.
  Feature-tree inspection confirms no runtime thiserror/std activation in no_std.
  Its proc-macro dependencies build for the host.
- `experimental-hardware` gates the embedded Rust test driver and enables
  portable atomics. Its generated non-Copy container fixture links without an
  allocator on thumbv8m.main-none-eabihf and riscv32imac-unknown-none-elf.
  For thumbv6m-none-eabi, library compilation passes and the fixture deliberately
  fails to link without the application's critical-section acquire/release
  symbols. This is an intended diagnostic check, not a successful fallback link.
  No test installs a pretend platform implementation or validates physical hardware.
- Remaining semantic identity cases, broader allocation/performance validation and
  packaged-consumer testing remain. Container generation, composition, scheduling and unchecked access have the
  tested coverage recorded at the top of this document.
