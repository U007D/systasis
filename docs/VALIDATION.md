# Implementation validation

User-macro support inside registrations was deferred on 2026-09-15. Existing
macro-specific results below are retained as evidence for future work, not release
completion criteria. Systasis's own resolver and registered-type query macros,
including their dependency and ownership checks, remain in scope.

## Current generated-container checks

2026-09-22 gist comparison: value registrations can reuse an already declared
local/parameter type or an explicit cast type. The parser previously rejected
every omitted registration annotation. Five public-API tests now cover named
containers, ownership/Copy policy (including generic parameters), final overrides,
groups and namespaces on std/no_std. A downstream package names the generated
container without added generic parameters; a macro test checks identical output
to explicit annotation. Existing compiler tests retain a diagnostic for unknown
or shadowed types instead of guessing. These are partial inference support, not
general inference of arbitrary expressions or opaque types. The selected library,
compiler, value, namespace/group and cross-crate tests and targeted Clippy pass on
both backends. No unsafe code, dependency, compiler feature or documented syntax
changed; examples continue to supply explicit registered types.

2026-09-22 gist comparison: resolution errors now implement the source's Clone,
Copy, Eq and PartialEq requirements, retaining the later chosen public names and
thiserror messages. The new public trait-bound/equality test failed before the
change. `errors`, `storage` and `container_access` tests and targeted Clippy pass
on std/no_std. This omission was not represented in the derived R01–R15 inventory;
that inventory alone does not establish complete coverage of the gist.

2026-09-22: type-query cycle diagnostics now identify the closed registration
path, completing the R06 diagnostic requirement previously implemented only for
runtime query cycles. Compiler cases check self-cycles and a two-registration
cycle reached through blocked dependents; the preceding generic diagnostic failed
the new expectation before the fix. The workspace library tests and the selected
`container_compiler`, `initialization_order`, `concurrent_container`,
`owned_container` and `namespaces` suites pass on std/no_std, as does targeted
Clippy with warnings denied. No lookup semantics, runtime, dependencies, unsafe
code or compiler features changed. The known full-suite compiler failures below
remain outside this targeted verification.

2026-09-22: `concurrent_container` verifies R04/R07 through generated methods on
a Sync container holding a non-'static borrowed payload. Scoped threads verify
compatible reads/clones, rejection of incompatible reads/writes/takes, independent
field access, transfer of a write guard, visibility of its mutation, and ownership
transfer followed by consumed errors. A channel handshake orders the observations;
a watchdog releases held guards before reporting a blocking regression. The test
and targeted Clippy pass on std/no_std with the installed nightly. This adds
integration evidence, not a runtime, dependency, unsafe-code or toolchain change.

2026-09-22: `initialization_order` verifies R06 through observable build
initializer execution: frozen dependency layers, source order within a layer,
waiting for the deepest dependency, type-query-only dependencies, and final
override selection/source position. The compiler driver also verifies that a
two-registration cycle reports the cycle without a blocked downstream dependent.
All three behavior tests and `container_compiler` pass on std/no_std with the
installed rustc 1.97.0-nightly (4b0c9d76a, 2026-05-10); targeted Clippy passes
with warnings denied. These close an integration-coverage gap; no generator,
runtime, dependency, unsafe code or toolchain change was needed.

2026-09-21, after removing the date pin (6516e3a): the installed floating
`nightly` reports rustc 1.97.0-nightly (4b0c9d76a, 2026-05-10). It was not updated.
The full default workspace test build fails with E0283 (`__NativeClosure: Send`)
in `capture_generic_array_remainder` and `native_child_context`; therefore no
full-suite success is claimed on this compiler. Neither test was changed,
disabled or made an expected failure. No compatibility investigation was started.

The explicitly selected basic-operation suites below pass on both backends:
241 tests checked-only and 248 with `resolve_unchecked,experimental-hardware`,
with zero failures and one intentionally ignored parser test per configuration.
The optional run also checks embedded linking. All twelve guide doctests and
all three existing application examples pass on both backends. Workspace checking,
formatting, and library/example Clippy with warnings denied pass; this is not
all-targets Clippy. Logs: `/private/tmp/systasis-basic-acceptance.uyX13L/`.
No unsafe/dependency change occurred; the Miri trigger does not apply.

2026-09-21 work priority: the R01–R15 inventory below tracks basic required
operations. Its representative coverage is separate from complete input-form
compatibility. Further Copy-alias, type-spelling and capture/cfg combination
research is deferred while demonstrating the basic container. Preserve known
limitations without turning them into new annotation requirements or additional
basic-completion gates. This is a prioritization change, not new test evidence
or a claim that the deferred inputs now work.

2026-09-17: explicit generic Copy bounds now ignore parentheses and invisible
type groups, without confusing tuples/wrappers with their arguments. New public
API tests cover repeated reads, ordinary references, named generic containers,
auto traits and cross-crate calls; negative fixtures preserve the explicit
whole-type rule and absence of ownership-taking methods. The selected workspace
library, generic-policy, child-policy and compiler suites pass 106 tests per
backend (one ignored), and all-targets Clippy passes on std/no_std with Rust
warnings denied. This is targeted validation, not a new full release-matrix run.
Renamed Copy imports and differently spelled type aliases remain unresolved.
No dependency, unsafe operation or compiler feature changed; no Miri trigger.

The 2026-09-16 post-integration toolchain check left production on the then-pinned nightly. The
isolated gate-removal comparison verifies sixteen expected outcomes on installed
stable/nightly and std/no_std: typed owned/lending captures pass; inferred owned
and typed destructuring captures need TAIT. See [NIGHTLY.md](NIGHTLY.md) for the
distinction between the modified research copy and supported production. All
twelve standalone builder tests pass on stable. The current owned, scopes and
quick_start application examples also run on pinned nightly with both backends.
This follow-up changes documentation only, not runtime/generator behavior.

At b21a6fd/e43bd81, checked std/no_std each pass 414 tests; feature-enabled
std/no_std each pass 421 (`resolve_unchecked,experimental-hardware`). Every
configuration has zero failures and four intentionally ignored tests. The totals
include twelve executable guide examples. Both feature-enabled all-targets
Clippy runs and both workspace Rustdoc builds pass with Rust warnings denied;
formatting and diff checks pass. Cargo still reports its existing unused-Spin
manifest warning on std.

Separately selected checks pass: extracted-package consumers on both backends
(now including a moved builder composing a child), 2,560 parser mutations,
nine code-generation workloads with three samples per backend, and ten release
runtime comparisons per backend. Allocation regression tests remain green.
Embedded native-CAS compile/link and missing-platform-fallback diagnostics pass;
no physical hardware was executed. Logs are `/private/tmp/systasis-builder-*.log`.
Performance checks validate their workloads, not a timing regression threshold.

The 2026-09-16 core-feature audit found and completed the missing unbuilt-builder
lifecycle. `tests/builder.rs` covers deferred execution, moved builders,
abandonment, input ownership/borrowing, overrides and immediate failure cleanup.
`tests/builder_compiler.rs` checks two successful programs and seven intended
compiler rejections per backend, including pre-build resolution, repeated build,
post-build registration, owner escape and conflicting input borrows. Twelve
runtime-helper tests cover the underlying safe consuming initializer.

The initial-release operations have implementations and representative tests:

| Contract group | Implemented operation | Representative evidence |
| --- | --- | --- |
| R01 | Concrete named container and consuming builder | `builder`, `builder_compiler`, `generic_cross_crate` |
| R02 | Stored Copy/consumable values and generic bound policy | `container_access`, `generic_container`, `policy` |
| R03 | Default/custom constructors, owned captures and returned borrows | `fresh_container`, `custom_constructor`, `capture_patterns` |
| R04 | Nonblocking ownership/shared/mutable access and guard lifetimes | `concurrent_container`, `storage`, `compiler`, `unsynchronized` |
| R05 | Named/nested child scopes and aliases | `child_aliases`, `nested_children`, `child_borrows` |
| R06 | Final overrides, dependency layers and build lifecycle | `initialization_order`, `owned_container`, `builder`, `container_compiler`; macro `graph` unit tests |
| R07 | Natural/requested auto traits, local tracking and async guards | `concurrent_container`, `container_compiler`, `generic_container`, `async_guards` |
| R08 | Explicit stored-value cloning | `container_access`, `storage` |
| R09 | Opt-in single/group dyn access and type queries | `dyn_container`, `dyn_groups`, `dyn_type_query` |
| R10 | Exact fallible constructor returns and Fallible conversions | `custom_constructor`, `fallible` |
| R11 | Build error selection/inference and failure effects | `build_inference`, `failure_effects`, `builder` |
| R12 | Feature-gated synchronized unchecked access | `unchecked`, `child_unchecked`, `compiler` |
| R13 | Namespaces, groups and container-only lookup | `namespaces`, `trait_groups`, `resolver_scope` |
| R14 | no_std, allocation-free workloads and portable atomics | `allocations`, `embedded_targets` |
| R15 | Compiler, package, safety and performance verification | `packaged_consumer`, `performance`, `codegen_scaling`; `SAFETY.md` |

Test names in the table refer to files under `tests/` unless noted. This inventory
is feature coverage, not a claim that every Rust input form works. Capture and
type-bound recognition gaps remain documented; further array-alias hardening,
user macros, stored internal borrows and physical-board execution are deferred.
No new dependency, unsafe operation or compiler feature was introduced by the
builder; the agreed Miri rerun triggers do not apply to this change.

At 4239ab8, all four full host configurations pass: checked std/no_std each
393 tests; `resolve_unchecked,experimental-hardware` std/no_std each 400 tests;
zero failures and four explicitly ignored per configuration. These totals include
the eleven guide doctests and four failure-effect tests. Both feature-enabled
workspace/all-targets Clippy runs and both workspace Rustdoc builds pass with
Rust warnings denied. Cargo separately reports the existing unused-Spin manifest
warning in the std configuration. Embedded compile/link checks pass, without
executing physical hardware.

The separately selected packaged-consumer check passes on extracted std/no_std
archives, including both license texts and capture-diagnostic examples; no crate
was published. The extended deterministic parser corpus passes 2,560 mutations.
Scaling and release runtime checks are described below. Full-matrix, Clippy,
Rustdoc, scaling and runtime logs: `/private/tmp/systasis-release-validation.USP1t1`.
This update changes documentation and safe tests only; no Miri trigger applies.

The opt-in scaling driver now keeps each child borrow independent of its child's
backing lifetimes. Its previous `&'a AppContainer<'a>` fixture failed at depth two
because the projected child storage is invariant; an ordinary invariant Rust
control fails with the same coupled lifetime and succeeds with separate lifetimes.
The corrected driver checks/links/runs all nine workloads, three samples each,
on both backends, including nesting depths 1/2/3 and named scope parameters.
Both release runtime-baseline runs also pass their ten workloads. Measurements
ran alongside other validation; they are not controlled performance comparisons.
No generator/runtime change was needed.

Four new failure-effect tests pass on std/no_std. A failed outer build does not
restore a consumed child value; a failed fresh constructor leaves its dependency
consumed. Values moved into an error live until that error is dropped. A child
guard moved into a build error survives the failed outer container, keeps taking
and mutation contended, and releases access when the error is dropped.
Both targeted Clippy runs pass with warnings denied; only tests were added.

The build-failure example passes on std/no_std and keeps the caller's borrowed
input usable after an initializer returns Err. The guide now has eleven executable
doctests per backend and states the pinned nightly requirement and local no_std
dependency configuration. Cleanup/no-rollback and unchecked method availability
text describe the existing contract; no implementation changed.

The trait-group guide example executes combined dynamic type queries during
build, then uses both dynamic and concrete access to the same stored value.
All ten doctests pass on std/no_std, including consumption after the example's
temporary build and caller borrows have ended.

The resolver guide now documents exact value/borrow return types, generic Copy
selection and stored-value cloning. Its two additional examples pass on std/no_std,
bringing the guide to nine executable doctests per backend. Workspace Rustdoc
builds with warnings denied. The availability table was checked against the
generator and existing behavior tests; this adds no API or runtime change.

The public guide now includes executable local-namespace and subcontainer
examples. Seven doctests pass on std/no_std, covering `_from` queries,
`_in_name`/`_in_default` methods, borrowed composition and a named child scope
passed to an ordinary function. These document existing behavior; generation
and runtime code are unchanged.

The accepted temporary annotation for an array reference extracted from a generic
tuple alias now has a runnable usage example and two integration regressions.
Five doctests and both new tests pass on std/no_std. They preserve exact capture
types, repeated calls, nameable containers, external element lifetimes and
unconstrained ownership/auto traits for an uncaptured tuple field. Both targeted
Clippy checks pass with warnings denied. This documents the accepted workaround;
automatic inference for that nested case remains deferred. Generator and runtime
code are unchanged.

2026-09-15: whole-sequence alias captures retain their original array or slice
type. Eleven integration tests include generic elements, nested reference layers,
empty arrays/slices, explicit shared/mutable local borrows, returned external
references, named containers, repeated lending, destruction and Send/Sync policy.
The explicit-borrow correction projects from the borrowed source type rather
than borrowing an associated-type output, preserving implied lifetime bounds.
Three additional compiler controls reject local-reference escape, mutation of
a shared-borrowed source and movement of an exclusively borrowed source.
The four full std/no_std checked/feature-enabled configurations pass (380 tests
per checked configuration; 387 with resolve_unchecked,experimental-hardware;
zero failures and four explicitly ignored each). Both feature-enabled
workspace/all-targets Clippy runs pass with Rust warnings denied. No unsafe,
dependency or compiler feature changed; Miri's agreed triggers do not apply.

2026-09-15: generic owned array-alias remainders in irrefutable patterns use
native capture storage without a user macro. Four integration regressions check
exact remainder type, repeated calls, independent head ownership, once-only
destruction, Send/Sync, and preserved owned-capture lending beside borrowed slice
aliases (including a hidden static borrow). Three capture-planning tests check
fallback selection, let-else/whole-slice preservation and parameter patterns.
Both full feature-enabled std/no_std workspace suites pass: 370 tests passed,
zero failed and four explicitly ignored per backend. Both feature-enabled
workspace/all-targets Clippy runs pass with Rust warnings denied; Cargo still
warns that Spin is unused in the std manifest configuration. Formatting passes.
No runtime unsafe code or dependency changed. Remaining capture limits are
documented in CAPTURE_LIMITS.md; this is not complete generic-remainder support.

The later documentation correction explains the borrowed-input lifetime cause
and recommends a non-borrowing implementation as a coarse workaround, without
claiming complete borrowed-constructor support or diagnostic coverage. Both
backend diagnostic drivers still pass their ten compiler outcomes, including
the explicit E0521-without-guidance control, and both helper integration tests
pass per backend. The separately run packaged-consumer check now also compiles
and runs the owned-input workaround on std/no_std; artifacts are under
`target/packaged-consumer/5119-1789246197217629000`. Workspace Rustdoc builds.
Both feature-enabled workspace/all-targets Clippy runs pass with Rust warnings
denied. No runtime or code-generation behavior changed.

At 775a901, both full feature-enabled std/no_std workspace suites pass with
363 tests passed, zero failed and four explicitly ignored per configuration
(including doctest summaries). Both checked-only workspace suites also pass:
357 passed, zero failed and four explicitly ignored per backend. Both
feature-enabled workspace/all-targets Clippy runs pass with Rust warnings denied.

The native-capture diagnostic driver checks ten compiler outcomes per backend:
two local-borrow E0597 errors include the source-excerpt remedy; elided-parameter
E0521 and output-mismatch E0308 controls do not. Six compiling/running cases
preserve owned/static captures, unused short borrows, caller macro semantics,
generic external lifetimes and the ordinary-function rewrite using the original
borrowed input. These are compiler-driven checks, not capture-token guesses.
The helper integration also passes beside plain local macro definitions,
including a same-name macro and function. Macro-analysis controls retain
conservative handling of invocations and potentially transforming attributes.
The diagnostic is not a structured compiler note or an IDE quick fix; its exact
scope is documented in [CAPTURE_LIMITS.md](CAPTURE_LIMITS.md).
The separately executed ignored packaged-consumer test passes on both backends:
it checks the remedy against extracted crate sources, then compiles and runs
the rewrite with the original borrowed input. Archives and consumer diagnostics
are retained under `target/packaged-consumer/83162-1789244898176069000`.
These safe changes add no unsafe code, dependency or compiler feature. Miri was
not rerun because none of its agreed triggers applies.

After 96f67bc/2f66f7a, all four checked/feature-enabled std/no_std workspace
suites pass. Extracted-package consumers pass on both backends, including the
nested borrowed-child cross-crate regression. Both feature-enabled all-targets
Clippy runs pass after the generator change.

The native owned generic-array remainder regression separately passes tests and
Clippy on std/no_std. It checks exact array type, retained Send/Sync bounds,
uncaptured head ownership and exactly-once element destruction. It deliberately
contains a macro; it is not evidence for the plain-body or borrowed-tail cases.

Native constructors now receive owned tuples of restricted child borrows,
separating backing lifetimes from temporary descriptor borrows. Eight source tests
pass per backend: private non-static child payloads, distinct sibling payload
lifetimes, nested owned/externally borrowed payloads, synchronized/local write guards, and native
forwarding to a reconstructed constructor. Synchronized guard destruction on
another thread and explicit outer child-reference lifetimes are included.
The existing 22 constructor tests and native
compiler driver also pass on both backends. Scope exclusion controls run with
both native and reconstructed bodies, including transitive nested consumption.
Macro tests pass 71 with one explicitly ignored corpus; two new AST checks
distinguish selected-child descriptor generation from tuple forwarding.
These safe code-generation changes add no dependency or unstable feature.
Nested externally borrowed payloads now pass without adding source lifetime bounds.
Stored child tuples and borrowed masks defer their associated-type projections
behind named helper types. The mask operations and scoped forwarding are unchanged.
Both full feature-enabled backend suites pass after this integration, including
scope exclusions, cross-crate naming, private payloads and allocation controls.
Both feature-enabled workspace/all-targets Clippy runs also pass with Rust
warnings denied.
All four checked/feature-enabled std/no_std workspace suites pass after the
explicit-lifetime fix, as do both feature-enabled all-targets Clippy runs with
Rust warnings denied. Cargo's separate std-only unused-Spin warning remains.
The cross-crate compiler test now adds a child borrowing local caller data,
native parent construction, concrete container/scope parameters, returned guard
contention and release; it compiles with warnings denied and runs on both backends.
It also composes an exported branch containing that borrowed child, then resolves
through two child levels from a native parent in a separate crate. Concrete
container and restricted-scope parameters retain the external payload lifetime;
independent child access remains contended until the returned guard is dropped.
Extracted-package std/no_std consumers also pass at 5746ec1. The allocation
suite now has sixteen passing cases per backend: two added workloads observe
no allocator calls while constructing nested native child contexts, repeatedly
returning guards and destroying all owners, with synchronized and local storage.
The allocator instrumentation and unsafe code are unchanged.
Two additional native compiler rejections require `Send` or `Sync` on a returned
local child write guard; both fail with E0277 on both backends. The unchanged
source passes as a runtime test, and the synchronized counterpart transfers its
guard between threads. Targeted std Clippy passes with warnings denied.
Three generic child regressions also pass tests and Clippy on both backends:
private concrete generic payloads, explicit child-backing output lifetimes, and
reconstructed/native constructor chains declared in reverse dependency order.
Both chains retain their child guards and restore mutable access on drop. A fourth
regression now checks authored type, lifetime and const names against generated
stored-child parameters; it reproduced E0403 before the reserved-prefix correction
and passes in both feature-enabled backend suites afterward.
The hidden `BorrowedBy` mask helper defers an associated-type projection and
forwards both mask operations unchanged. Four runtime-library tests pass on
std/no_std, including exact associated-type equality and local/nested/union
restriction controls. Generated child masks now use this helper as part of the
nested-lifetime correction above.

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

Reproduce with the repository's selected toolchain using `cargo test --offline --locked --test allocations`,
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
covered. Run the larger corpus with `cargo test -p systasis-macros
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
Run `cargo test --release --test performance --offline --locked --
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
law. No timing threshold is enforced. Run `cargo test --test codegen_scaling
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

Run from the repository root to select the installed `nightly` channel through
`rust-toolchain.toml`. The macro package currently requires nightly; historical
results above describe their named revisions and compilers. Record `rustc --version`
with new results. The complete-suite commands below currently encounter the two
compile failures documented above on the installed 2026-05-10 compiler.

```sh
cargo test --workspace --offline --locked
cargo test --workspace --no-default-features --offline --locked
cargo test --workspace --features resolve_unchecked,experimental-hardware --offline --locked
cargo test --workspace --no-default-features --features resolve_unchecked,experimental-hardware --offline --locked
cargo clippy --workspace --all-targets --features resolve_unchecked,experimental-hardware --offline --locked -- -D warnings
cargo clippy --workspace --all-targets --no-default-features --features resolve_unchecked,experimental-hardware --offline --locked -- -D warnings
cargo fmt --all -- --check
cargo check --no-default-features --target thumbv8m.main-none-eabihf --offline --locked
cargo check --no-default-features --target riscv32imac-unknown-none-elf --offline --locked
```

The bounded basic-operation run used this existing-test selection. Repeat with
`--no-default-features` for no_std, and with
`--features resolve_unchecked,experimental-hardware --test embedded_targets`
for each backend's optional-feature run. This selection does not replace or
silently weaken the complete-suite commands above.

```sh
cargo test --workspace --offline --locked --lib \
  --test builder --test builder_compiler --test container_access \
  --test generic_container --test fresh_container --test custom_constructor \
  --test owned_container --test storage --test compiler --test container_compiler \
  --test namespaces --test trait_groups --test nested_children --test child_borrows \
  --test resolver_scope --test unsynchronized --test async_guards \
  --test dyn_container --test dyn_groups --test dyn_type_query --test fallible \
  --test build_inference --test failure_effects --test allocations \
  --test unchecked --test child_unchecked
cargo test --workspace --doc --offline --locked
cargo run --example quick_start --offline --locked
cargo run --example owned --offline --locked
cargo run --example scopes --offline --locked
cargo clippy --workspace --lib --examples --offline --locked -- -D warnings
```

The doctest, example and Clippy commands also passed with `--no-default-features`.

Run Miri when changing systasis-owned unsafe code, adding a dependency, or
investigating a specific dependency concern. Use a compatible installed Miri
toolchain for these commands; `+nightly` below is a local alias, not a reproducible
pin. Safe-only documentation and generator changes do not trigger a Miri rerun.

```sh
cargo +nightly miri test -p systasis --lib --offline --locked
cargo +nightly miri test -p systasis --test storage --offline --locked
cargo +nightly miri test -p systasis --test storage --no-default-features --offline --locked
cargo +nightly miri test -p systasis --test reservation --offline --locked
cargo +nightly miri test -p systasis --test reservation --no-default-features --offline --locked
```

Offline commands require cached dependencies; cross-target commands require
the corresponding installed target libraries.
Repeat the storage and reservation Miri commands with
`MIRIFLAGS=-Zmiri-tree-borrows` for the recorded second aliasing model.
An ancestor Cargo configuration in the development environment wraps rustc
with Clippy. Runs here clear `RUSTC_WRAPPER`, `RUSTC_WORKSPACE_WRAPPER`,
`CARGO_BUILD_RUSTC_WRAPPER`, and `CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER` for each
command. No global configuration was changed.

## Current boundaries

Local package validation at 378d2f2 found and fixed missing license texts in the
proc-macro archive. At that revision, `cargo +stable package --offline -p systasis-macros`
included both texts and verified the extracted package. Runtime packaging and
an extracted no_std build passed with a command-line crates.io patch pointing at
that extracted macro package. This was local artifact validation, not evidence
that the unpublished dependency can be downloaded from a registry. No package
was published; no persistent patch or lockfile change was retained.

`tests/packaged_consumer.rs` (54c354e) now stages tracked sources, creates and
extracts both actual crate archives, checks both license texts in each, and
compiles/runs downstream std and no_std-library consumers. It checks nameable
AppContainer/child scope types, child aliases and checked contention/consumption.
The unpublished macro dependency is patched to the extracted macro package only
for these commands. Source manifests and Cargo.lock are verified unchanged.
Run `cargo test --test packaged_consumer --offline --locked -- --ignored
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
- Known capture/type-bound recognition gaps and broader performance coverage
  remain. The core-operation inventory and current verification are recorded at
  the top of this document; they do not establish every input form or workload.
