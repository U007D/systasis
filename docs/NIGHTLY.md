# Temporary nightly toolchain

The project pins `nightly-2026-09-06`. Nightly was accepted temporarily to
complete the crate while preserving its concrete `AppContainer` and named
subcontainer APIs. Investigate stable replacements after integration.

## Feature inventory

| Feature | Purpose | Location |
| --- | --- | --- |
| `type_alias_impl_trait` | Name compiler-inferred native closure storage without exposing closure type parameters in `AppContainer`. | Generated private constructor storage; integration in progress. |
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

Record production integration evidence in `VALIDATION.md`; preliminary compiler
probes outside this package do not establish full constructor-macro support.
