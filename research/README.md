# Stable Rust research archive

On 2026-11-17 the user requested integrating this work on `stable-impl`.
The branch now contains the stable implementation as well as these original
probes. See [stable support and API migration](../docs/STABLE.md) for the current
API and [validation](../docs/VALIDATION.md) for integration evidence.

## Original parking decision

On 2026-11-15 the user identified the proposed generic container/error API as a
usability regression from the nightly API and requested that this work be parked
on `stable-impl`, leaving `main` untouched.

The initial branch commit archived the investigations and runnable probes and
versioned both packages `0.2.0`. It left production implementation and toolchain
unchanged. The branch is based on main commit
`2122f2006cc52ebfe06e141b131ac9240f19592f`.

- [Stable replacement investigation](stable-replacements/README.md): named factory
  metadata, earlier candidates and their limitations.
- [Stable generic representation](stable-rpit/README.md): inferred native closure
  storage and inline build errors using stable return-position `impl Trait`.
- [API minimization findings](stable-rpit/API-MINIMIZATION.md): why the prototype
  uses a shared error parameter and a result alias; minimal API changes remain
  unproven.

At that time the prototype's generic API was not selected for integration.
The subsequent implementation request applies to `stable-impl`; `main` retains
its existing API and toolchain.
