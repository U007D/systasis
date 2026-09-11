# Temporary nightly toolchain

The project pins `nightly-2026-09-06`. Nightly was accepted temporarily to
complete the crate while preserving its concrete `AppContainer` and named
subcontainer APIs. Investigate stable replacements after integration.

As checked on 2026-09-10, TAIT has no announced stabilization release. Its
[tracking issue](https://github.com/rust-lang/rust/issues/63063) lists stabilization
of the next-generation trait solver as a prerequisite. The Rust team's
[2026-08-21 update](https://blog.rust-lang.org/2026/08/21/enabling-next-solver-on-nightly/)
targets that solver for the coming months; it does not announce a TAIT date.

## Feature inventory

| Feature | Purpose | Location |
| --- | --- | --- |
| `type_alias_impl_trait` | Name compiler-inferred native closure storage without exposing closure type parameters in `AppContainer`. | Generated private constructor storage. |
| `allow_internal_unstable` | Permit the generated opaque aliases and their defining functions without requiring application-level feature annotations. | Procedural-macro entry points. |

These are compiler/code-generation features, not runtime dependencies or unsafe
operations. Existing guard implementations need no unstable mapped-guard APIs.
Build-error inference does not emit `!`; explicit `.build::<!>()` uses the
selected compiler's support for that spelling.

## Verification and removal

Retain native capture ownership, lazy/repeatable calls, returned guards, natural
auto traits, failed-build cleanup, and cross-crate container/subcontainer naming
when replacing either feature. Keep current returned borrows into reconstructed
owned capture state; native closures alone do not support that case.

Production native closure tests cover owned macro captures, explicit external
lifetimes, returned dependency guards, subcontainers and cross-crate privacy.
Remaining cases are in `CAPTURE_LIMITS.md`; these tests do not establish full
constructor-macro support. No other unstable feature has been enabled by this
integration.
