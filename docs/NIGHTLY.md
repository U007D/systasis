# Temporary nightly toolchain

The project selects the floating `nightly` channel, not a dated toolchain.
Nightly was accepted temporarily to complete the crate while preserving its
concrete `SystasisContainer` and named subcontainer APIs. Record the actual compiler
version with validation results; different nightly versions may behave differently.
Investigate stable replacements after integration.

Selecting `nightly` uses that installed channel; it does not update an existing
installation. Updating it is a separate `rustup update nightly` operation.

User macros inside registrations were deferred from the initial release on
2026-09-15. Native closure storage also supports macro-free capture cases, so
that deferral alone does not establish that nightly can be removed. No compiler
driver or rustc-dev component is required to use the production crate.

As checked on 2026-09-16, the Rust documentation still treats
[type-alias impl Trait (TAIT)](https://doc.rust-lang.org/unstable-book/language-features/type-alias-impl-trait.html)
as unstable. Its [tracking issue](https://github.com/rust-lang/rust/issues/63063)
remains open and lists next-generation trait-solver stabilization as a prerequisite.
This package assumes no stabilization date and does not claim a stable MSRV.

## Feature inventory

| Feature | Purpose | Location |
| --- | --- | --- |
| `type_alias_impl_trait` | Name compiler-inferred native closure storage without exposing closure type parameters in `SystasisContainer`. | Generated private constructor storage. |
| `allow_internal_unstable` | Permit the generated opaque aliases and their defining functions without requiring application-level feature annotations. | Procedural-macro entry points. |

These are compiler/code-generation features, not runtime dependencies or unsafe
operations. Existing guard implementations need no unstable mapped-guard APIs.
Build-error inference does not emit `!`; explicit `.build::<!>()` uses the
selected compiler's support for that spelling.

`allow_internal_unstable` enables only TAIT in generated code; configuration
selection has no independent unstable mechanism. The temporary builder closure
is inferred locally and consumed at build time, so it does not need TAIT.
The stored constructor closure is different: its type becomes part of the
module-scope, concrete `SystasisContainer`, including when its captures are inferred.
Rust [closure types](https://doc.rust-lang.org/reference/types/closure.html)
are anonymous; the generated opaque alias currently supplies that field's name.

## Verification and removal

The 2026-09-16 comparison used installed stable 1.98.1 and nightly-2026-09-06.
In an isolated source copy with only the nightly permission attributes removed,
typed owned captures and lending from owned captures compile and run on std/no_std.
Inferred owned captures and typed struct-destructuring captures still fail with
TAIT E0658. All four cases pass on the original nightly implementation; sixteen
expected outcomes were checked. This includes ordinary macro-free code, not just
the deferred user-macro feature. The twelve standalone builder tests also pass
on stable, including build-error inference.

Those stable successes describe a modified research copy, not a supported stable
package: the unmodified macro crate fails stable compilation at its feature gate.
That comparison retained the dated nightly pin; the pin was removed on
2026-09-21 without changing capture support or concrete container signatures.
Reproduction source and logs are preserved in the parent
workspace's `research/stable-boundary/` directory.

Retain native capture ownership, lazy/repeatable calls, returned guards, natural
auto traits, failed-build cleanup, and cross-crate container/subcontainer naming
when replacing either feature. Keep current returned borrows into reconstructed
owned capture state; native closures alone do not support that case.

Production native closure tests cover owned macro captures, explicit external
lifetimes, returned dependency guards, subcontainers and cross-crate privacy.
Remaining cases are in `CAPTURE_LIMITS.md`; these tests do not establish full
constructor-macro support. No other unstable feature has been enabled by this
integration.
