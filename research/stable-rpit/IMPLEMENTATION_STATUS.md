# Stable workaround investigation — 2026-11-15

## Current status: integrated on `stable-impl`, 2026-11-17

The user requested finishing the stable implementation on `stable-impl`.
This supersedes the earlier parking decision for this branch. The integrated
stable generic representation preserves registration syntax, owned inferred
captures, inline inferred errors, infallible `let Ok(...)`, natural auto traits,
cleanup, and named/nested subcontainers. `main` remains unchanged.

The concrete failure is that ordinary `cargo +stable check` rejects the macro
crate's nightly feature gate. Acceptance is the integrated public API passing
the std/no_std workspace suites, downstream/compiler tests, examples and docs
on stable with bootstrap disabled and unstable features forbidden. No new
dependencies or unsafe storage mechanisms were added.

Acceptance is complete on Rust 1.100.0 with bootstrap removed and unstable
features forbidden: the std and no_std workspace suites each pass 507 tests,
zero failures and four pre-existing ignored tests, including all 17 usage-guide
doctests and downstream/compiler fixtures. Warnings-denied Clippy passes on
both backends; allocator-free contracts compile on the host and
`thumbv8m.main-none-eabihf`. Public generic naming and migration examples are
documented in [STABLE.md](../../docs/STABLE.md), with detailed evidence in
[VALIDATION.md](../../docs/VALIDATION.md). No implementation work remains for
this request; the sections below retain the earlier investigation history.

## Previous status: parked

The user judged this approach a usability regression from the nightly API and
requested archiving the work on `stable-impl`, with main untouched. The research
is preserved here, not selected for integration. Do not resume it without a new
user request. The following sections retain the investigation's historical scope
and findings; they do not describe pending implementation work.

## Investigation scope

The user explicitly reopened stable-toolchain research and rejected bootstrap.
Investigate with stable rustc, bootstrap removed and unstable features forbidden.

Accepted behavior being checked: inferred inline initialization errors, original
source identity and auto traits, and irrefutable `let Ok(container)` whenever the
inferred build error is never, including propagation from an infallible source.
Native inferred constructor state must remain owned, lazy and repeatable.

Current failure: production requires TAIT to give hidden closure and error types
module-scope names. Existing RPIT probes lose the infallible-pattern guarantee.

New candidate: use a const function taking a reference to a factory function item
to read an inferred result-kind associated constant without executing the factory.
Use that constant in a stable associated-type projection containing return-position
`impl Trait`, selecting actual never for infallible builds and an opaque source
for fallible builds. Check whether this preserves the required pattern and error
behavior across crates before investigating container representation.

Acceptance: stable compiler/runtime evidence for inferred owned constructors,
infallible source propagation, mixed fallible sources, error downcasts, ownership
cleanup and positive/negative auto traits. Record any public-name/API limitations;
research does not adopt an API change or alter production.

## Result

The stable candidate passed core-only no_std and no_std+alloc provider/downstream
checks. The same representation preserves inferred captures, opaque factory and
error outputs, mixed inline errors, infallible propagation, source identity,
cleanup, borrowed lifetimes and positive/negative auto traits. Compiler negatives
also establish the exact naming incompatibility with bare public container/error
types. See README.md; the driver regenerates evidence in a temporary directory.
Production integration is not selected or
performed; retaining bare concrete names remains unresolved.

## API-minimization follow-up

The user asks why Result became BuildResult and whether a generic error alias
can keep the SystasisContainerError name. Check explicit Result signatures against
the existing provider, then test a selector whose infallible output witness has
only one valid type. This might remove the shared error parameter from the
container and allow ordinary Result plus SystasisContainerError<impl Error>.
Acceptance: compile/run infallible and fallible downstream wrappers, preserving
irrefutable let Ok, source identity and natural auto traits without new annotations
on registrations. Keep the proven prototype unchanged while checking this design.

Follow-up evidence: direct std::Result plus SystasisContainerError<impl Error>
works for the possibly-fallible downstream wrapper. The infallible equivalent
fails E0277 because its independent opaque payload becomes unconstrained when
selected away to actual never. Restricting the selector to one payload witness
also fails; it does not define that opaque payload. The existing shared-E alias
remains the verified uniform representation. No minimality claim is justified;
API-MINIMIZATION.md records the distinction and the reproduction fixtures.
