# Implementation validation

## Current generated-container checks

Native constructors now receive owned tuples of restricted child borrows,
separating backing lifetimes from temporary descriptor borrows. Seven source tests
pass per backend: private non-static child payloads, distinct sibling payload
lifetimes, nested owned payloads, synchronized/local write guards, and native
forwarding to a reconstructed constructor. Synchronized guard destruction on
another thread and explicit outer child-reference lifetimes are included.
The existing 22 constructor tests and native
compiler driver also pass on both backends. Scope exclusion controls run with
both native and reconstructed bodies, including transitive nested consumption.
Macro tests pass 71 with one explicitly ignored corpus; two new AST checks
distinguish selected-child descriptor generation from tuple forwarding.
These safe code-generation changes add no dependency or unstable feature.
Nested externally borrowed payloads remain a separate documented gap.
All four checked/feature-enabled std/no_std workspace suites pass after the
explicit-lifetime fix, as do both feature-enabled all-targets Clippy runs with
Rust warnings denied. Cargo's separate std-only unused-Spin warning remains.
The cross-crate compiler test now adds a child borrowing local caller data,
native parent construction, concrete container/scope parameters, returned guard
contention and release; it compiles with warnings denied and runs on both backends.
Extracted-package std/no_std consumers also pass at 5746ec1. The allocation
suite now has sixteen passing cases per backend: two added workloads observe
no allocator calls while constructing nested native child contexts, repeatedly
returning guards and destroying all owners, with synchronized and local storage.
The allocator instrumentation and unsafe code are unchanged.
Two additional native compiler rejections require `Send` or `Sync` on a returned
local child write guard; both fail with E0277 on both backends. The unchanged
source passes as a runtime test, and the synchronized counterpart transfers its
guard between threads. Targeted std Clippy passes with warnings denied.

The hidden child-context conversion preserves scope restrictions and backing
lifetimes. Four generated-scope tests and the two-test child-borrow driver pass
on std/no_std. Compiler controls reject clearing restrictions, accessing private
backing storage, restoring consumption, and extending either context or returned
guard to `'static`. Read/write guards outlive temporary descriptors and contexts,
retaining contention until dropped. Targeted std Clippy passes; this safe helper
adds no unsafe code or dependency. Constructor integration is recorded above.

Private borrowed child payloads now remain private when parent factory metadata
normalizes child scope projections. The regression first failed with E0446,
then passed with both payload and returned guard-containing result still private.
Fourteen targeted tests pass per backend (`child_private_payload`,
`child_container`, `child_generic_bounds`, `native_constructor`); the new test
also passes std Clippy with warnings denied. No storage, unsafe, dependency or
public-generic change was needed.

Capture-analysis fallback preserves successful typed reconstruction and delegates
unnameable captures to native Rust closures. Seven source tests cover inferred
owned/generic bindings, struct/tuple-struct and tuple-alias rest destructuring,
macro-created bindings, and function-local globs; they pass on both backends.
The former untyped-capture compiler rejection is now a positive fixture. Unit
tests separately check analysis failures, fallback from the original AST after
partial rewrites, and preservation of the owned-capture lending path.
The previous glob-shadowing rejection also becomes a compile-and-run positive:
Rust selects the imported function, the caller-owned String remains usable,
and repeat resolution returns the imported function's result. It compiles with
warnings denied and forbid(unsafe_code) on both backends.
Both full feature-enabled workspace suites pass after the fallback change and
glob fixture migration, as do both feature-enabled all-targets Clippy runs.
The three additional generic/macro-binding/tuple-rest cases pass targeted tests
on both backends and std Clippy with warnings denied.
At 2c03d50, both complete checked-only suites also pass, including the seven
fallback tests. The four host configurations therefore pass after fallback;
separate ignored package/scaling/performance checks are not part of these runs.

The temporary nightly pin is `nightly-2026-09-06` (28c6cd5). The 13 standalone
compiler drivers now use Cargo-reported artifacts (6f630c0), including split
metadata and link artifacts. Their targeted std/no_std suites and ignored
compiler-scaling execution checks pass on the new layout; the helper also
passes an independent stable Cargo-layout control. Existing exact diagnostic
codes and source-site checks remain, with two accepted renderings of one E0277
type-bound diagnostic. Nightly embedded compile/link checks now also pass on
thumbv8m.main-none-eabihf, riscv32imac-unknown-none-elf and thumbv6m-none-eabi.
This is not physical-board validation.

Native constructor integration passes eleven source-syntax tests per backend and
the cross-crate compiler driver: public/private result types, concrete container
naming, generics/external lifetimes, multiple child scopes, and four intended
privacy/ownership/auto-trait rejections. The inferred-capture regression checks
concrete container naming, repeated calls and required Send+Sync without a
capture annotation. The eleven existing custom-constructor
tests still pass, including returned borrows into owned captures. Independent
review found and verified a fix for rewriting `self` inside nested authored
items. No allocation mechanism, dependency or unsafe operation was added;
Miri was not rerun for this safe code-generation change. Remaining cases are
listed in CAPTURE_LIMITS.md, not counted as implemented.

The native mutable-guard regression checks repeated construction, shared and
exclusive contention while its returned RefMut is retained, mutation through
that guard, and successful access after the returned service is dropped.

The four std/no_std checked/feature-enabled workspace suites pass with the
native branch, as do both feature-enabled all-targets Clippy configurations.
The generated uninhabited-error correction is in 664390a; caller warnings remain
enabled. Cargo independently warns that Spin is unused in the std compilation;
it remains required by the no_std backend. The added native lifecycle workload
observes no allocator calls during build, repeated resolution and destruction.
Capture-annotation omission where rustc safely infers the types was accepted on
2026-09-10; documented examples remain explicitly typed.

The extracted-package driver also passes on the pinned nightly with both std
and no_std consumers. Its native constructor uses a private output type, typed
array capture, concrete AppContainer parameter and required Send+Sync, without
application feature attributes or unsafe code. Both package license texts and
unchanged source manifests/lockfile are checked; nothing is published.

The temporary reserved-name convention is documented in 7ce59f1. The three
`initializer_names` tests pass on stable with std and no_std: ordinary glob
imports preserve caller constants/type aliases, local and nested-child queries
consume their actual slots, untaken queries leave values available, temporary
input borrowing permits later reuse, and nested labelled breaks preserve
branch-sensitive moves. Both targeted Clippy runs pass with warnings denied.
These tests use supported names; they do not fix or guarantee diagnostics for
collisions with the reserved `__systasis_*` prefix. The research reproducer of
wrong-value lookup remains outside production. Updated usage doctests pass
on both backends (three each). No unsafe/dependency change triggered Miri.

At 020f06e, the full stable workspace suite passes in four host configurations:
default std, std with all features, no_std, and no_std with
resolve_unchecked,experimental-hardware. Both feature-enabled all-targets Clippy
runs pass with warnings denied. Rustdoc and the quick_start/scopes examples pass
on both backends. The ignored package and release-runtime drivers were run
separately on both backends. This records tested configurations, not completion
of the capture cases still listed in CAPTURE_LIMITS.md.

The crate-level [usage guide](USAGE.md), included by src/lib.rs, now has three
executable doctests: stored Copy/non-Copy access with a lazy captured constructor,
an exactly typed fallible constructor, and owned dependency injection. All three
pass on std and no_std. The standalone scope-injection example also runs on both
backends, passing a nameable restricted child reference to an ordinary function.
The requirements' complete quick example also runs on both backends, retaining
its private implementation types and inline registration shape.

Tuple-alias capture support (ee235a1/9d0f9d8) passes ten integration/driver checks on each
backend: nested shared/mutable binding modes, selected ownership and exact drops,
discarded overrides, authored generic parameters, and a separately compiled
consumer of a container with private captures. The negative fixture checks that
generated capture helpers remain private. A unit test generates an arity-64
projection from its authored pattern; implementations are not a fixed arity list.
Additional checks preserve explicit ref/ref mut bindings, lifetime/const
parameters, surrounding helper-like names and nested selected-leaf ownership.

Configured captures (d373c08/50c41e9) pass four integration tests per backend.
Rust selects active local annotations before capture analysis; statement
ancestors are selected before their descendants. A native Rust control and
generated regression retain invalid child predicates under disabled blocks.
Discarded conditional array elements and match arms remain rustc-owned.
This verifies the bounded support in [capture limits](CAPTURE_LIMITS.md), not
arbitrary configuration inside signatures or opaque registration tokens.
Constant-depth selection (50609ee) passes a Rust-driver fixture with 256
nested-conditional bindings and recursion_limit = 64 on both backends. It also
checks disabled malformed/invalid predicates against ordinary Rust controls,
enabled invalid-predicate errors, erased helper-name collisions, sibling modules
and nameable AppContainer signatures. No compiler invocation is added to normal
container expansion; the extra rustc calls belong to the test driver.

Array/slice alias projections (6864c69/c522b6b/11856e9) pass nine tests per backend, covering
generic array elements, concrete owned/borrowed array remainders, shared/mutable
slice remainders, exact drops and uncaptured element reuse. A named generic
container returns the same borrowed slice by pointer identity. Unsupported
alias shapes remain listed in [capture limits](CAPTURE_LIMITS.md).
The mutable Cell-array tail retains Send without requiring Sync; a shared-tail
compiler fixture fails for the expected Cell: !Sync reason. Both cases retain
the authored borrow mode rather than changing it to satisfy the assertion.

Closure-local glob checks (361cee5/9eaa1fb) cover imported constants/functions,
sibling captures, returned guards and precise ambiguity diagnostics. Eight tests
pass per backend. Independent review found that glob-imported items could replace
generated storage names even with mixed-site identifiers. Constructor queries now
access a private borrowed context through self, with anchored helper paths.
Regressions cover local slots, direct/indirect child queries, a caller initializer
binding named like generated child storage, and unrelated original generics.
A generated-AST check prevents adding blanket outlives bounds to those generics;
only the referenced storage types must outlive the call. The thirteen allocation
checks still pass per backend after this safe generated-code change.

Extracted-package consumers now combine configured local bindings, private
tuple-alias captures, named child scopes and a constructor query under a glob
import. Both std and no_std consumers compile and execute. These are local
archive checks with the unpublished macro dependency patched to its extracted
package; no registry publication or registry availability is implied.

Async guard checks (cc4ffe3/2603e3d) use safe manual polling without an executor
dependency. Both backends verify shared/exclusive contention while suspended,
release on cancellation, and guard release before suspension. Synchronized guard
futures satisfy Send for the tested payload. Compiler fixtures separately reject
local guards and &!Sync containers in Send futures, while a local Send container
and a future containing only its owned cloned result pass. Each negative check
matches an actual error at the expected fixture assertion.

Rest-capture support (96f3c3b/032725d) passes two integration tests per backend:
selected owned elements move/drop once; borrowed arrays/slices retain their
reference types. Unit coverage checks literal, named concrete and arithmetic
lengths, binding modes, invalid literal shapes, and generic expressions that must
not gain a generated subtraction. More complex const expressions remain a gap.

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

Container-only lookup (1a67b28/653426b) passes four tests per backend. Local,
named, nested, type and dyn queries ignore unrelated surrounding names. Missing
registrations do not fall back to outside traits or aliases; the Rust driver
checks the intended diagnostics and retains stderr under target/resolver-scope.
Runtime contention and consumption likewise return the selected slot's error
despite an available caller-owned value. These are accepted semantics, not
failed interface-alias support. Registration annotations retain Rust name resolution.

Implicit capture binding modes pass five integration tests per backend and two
additional capture-analysis unit tests. Explicitly typed reference-to-tuple/array
patterns preserve shared/mutable bindings, nested reference layers, caller input
reuse after container destruction, and returned references to captured data.
No new unsafe code, dependency or user annotation is introduced.

Capture elision (c1ce92c) passes four regressions on each backend: a captured
reference parameter, independent references including a nested generic capture,
and function-pointer/Fn signature lifetimes that must remain higher-ranked.
Nested dyn type metadata (21b7309) passes short intermediate-borrow and non-static
payload cases on both backends, with associated bindings and local storage.
These fixes add no caller annotation or unsafe code. The separate implied nested
child lifetime-bound case is fixed in a56d8ad: internal validation helpers receive
the same shared child reference types as the caller, retaining their implied
outlives bounds. The regression and an ordinary Rust control need no explicit
relationship between the child's two authored lifetimes.

`parse_properties` (b79dc75) expands ten accepted syntax fixtures twice and checks
repeatability plus parseable generated Rust. Its default mutation test checks
160 deterministic token mutations, accepting either repeatable diagnostics or
parseable generated output. The opt-in corpus checks 2,560 mutations with the
same fixed generator/seed; it also passes. This is bounded parser/generator
testing, not proof that all generated programs typecheck or that all tokens are
covered. Run the larger corpus with `cargo +stable test -p systasis-macros
--offline --locked parse_properties::extended_mutation_corpus -- --ignored`.

`tests/performance.rs` (0b4dddd) provides ten opt-in host comparisons against
handwritten use of the same storage primitives: Copy, local/synchronized read
and write guards, repeated constructors, and local/synchronized construction
with consumption or destruction. Each uses nine alternating pairs of 100,000
iterations after warmup, black-boxed inputs/results, checksum checks and exact
payload-drop counts. Output records toolchain, host, features and min/median/max
ns per iteration. Both runtime backends pass the release checks on the recorded
aarch64 macOS host. There is no speed threshold or hard real-time claim;
broader workloads remain separate verification work.
Run `cargo +stable test --release --test performance --offline --locked --
--ignored --nocapture`, then add `--no-default-features` before `--` for spin.

`tests/codegen_scaling.rs` (25bb42e) separately measures stable compiler invocations
for 1/8/32 flat registrations, 1/8/32 captured constructors each resolving one
shared stored dependency, and child nesting depths 1/2/3. It warms dependency
artifacts, then checks and builds/links distinct input crates for three samples
per case; each executable validates resolution and nameable container/scope types.
Both backends pass, including repeat resolution of each captured constructor.
The post-context-change runs executed concurrently with other validation work;
their timings are not controlled before/after performance comparisons.
Output records source/executable sizes and separate metadata
check and build/link durations, not expanded-token size or a universal scaling
law. No timing threshold is enforced. Run `cargo +stable test --test codegen_scaling
--offline --locked -- --ignored --nocapture`, adding `--no-default-features` for spin.

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
composition has the dedicated evidence recorded above. Resolver query names are
container-relative under the later clarification, not caller-scope Rust aliases.

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

`tests/packaged_consumer.rs` (54c354e) now stages tracked sources, creates and
extracts both actual crate archives, checks both license texts in each, and
compiles/runs downstream std and no_std-library consumers. It checks nameable
AppContainer/child scope types, child aliases and checked contention/consumption.
The unpublished macro dependency is patched to the extracted macro package only
for these commands. Source manifests and Cargo.lock are verified unchanged.
Run `cargo +stable test --test packaged_consumer --offline --locked -- --ignored
--nocapture`; the explicitly selected test always exercises both backends.
It is excluded from ordinary test runs because it stages/package-builds sources
and creates isolated downstream builds (about four seconds on the tested host).
The no_std library is executed by a std host binary, not on embedded hardware.

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
- Remaining capture cases, documentation completeness and broader performance
  validation remain. Container generation, composition, scheduling and unchecked access have the
  tested coverage recorded at the top of this document.
