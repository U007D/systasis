# Stable Rust research — parked

On 2026-11-15 the user identified the proposed generic container/error API as a
usability regression from the nightly API and requested that this work be parked
on `stable-impl`, leaving `main` untouched.

This branch archives the investigations and runnable probes. Both workspace
packages are versioned `0.2.0` on this branch; the stable port remains unimplemented.
Production macro and runtime source, third-party dependencies, public API and
toolchain are unchanged. The branch is based on main commit
`2122f2006cc52ebfe06e141b131ac9240f19592f`.

- [Stable replacement investigation](stable-replacements/README.md): named factory
  metadata, earlier candidates and their limitations.
- [Stable generic representation](stable-rpit/README.md): inferred native closure
  storage and inline build errors using stable return-position `impl Trait`.
- [API minimization findings](stable-rpit/API-MINIMIZATION.md): why the prototype
  uses a shared error parameter and a result alias; minimal API changes remain
  unproven.

The prototype's public generic parameters are not an accepted API change. Full
registration-macro integration and named subcontainers were not implemented.
Resume this work only on an explicit user request; retain the nightly API on main.
