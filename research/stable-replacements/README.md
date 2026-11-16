# Stable replacement investigation

Archived on `stable-impl` alongside the later [generic representation](../stable-rpit/README.md).
The user has [parked stable migration](../README.md); the findings below are historical.

2026-11-15. Production baseline: `2122f2006cc52ebfe06e141b131ac9240f19592f`.
Research only; no production macro, runtime, dependency, toolchain or API changes.

The user requested an investigation of replacements for the two uses of TAIT,
without `RUSTC_BOOTSTRAP`. Preserve concrete container naming, compiler-inferred
captures, inferred inline build errors, infallible `let Ok(...)`, ownership and
natural auto traits. Earlier closure-naming, generic-default and trait-interface
experiments are in the parent workspace's `research/native-closures/`; the
declaration error mechanism is in `systasis-macros/src/declaration.rs`.
The parent `AGENTS.md` and latest explicit user decisions govern those contracts.
The live requirements gist was attempted through both gist and raw URLs but was
unavailable to the web reader; this investigation does not amend its requirements.

## Finding

No demonstrated replacement preserves the complete current contract. This is a
bounded compiler investigation, not a proof that every stable encoding fails.

There is a concrete stable representation if factory types can be named from
their signatures: generate metadata beside a factory function, then use that
metadata for reconstructed capture fields and inline error variants. This keeps
the concrete container/error names and avoids boxed storage. It does not infer
the type of an arbitrary expression or name an opaque function return type.

`provider.rs` is a **hand expansion** of that representation, not an implemented
attribute or a Systasis integration. For example, a hypothetical attribute could
turn an already typed factory into a function plus a same-named module:

```rust,ignore
// Proposed syntax, not available in Systasis:
#[systasis::factory]
fn number(input: &str) -> Result<u8, ParseIntError> { input.parse() }

// Candidate emitted metadata:
mod number { pub type Error = ParseIntError; }
```

The existing signature supplies the error type; the caller would not write a
second error list. A real implementation would need visibility, generic, path,
collision and factory-registration handling. Requiring attributes on factories
or restricting initialization expressions changes the supported interface and
has not been adopted.

The tested no_std+alloc provider and downstream consumer establish that this
representation supports a concrete `&SystasisContainer` helper, repeatable fresh
calls, lending from named owned capture storage, inline mixed original errors,
`Error::source()` identity, a named error in a return signature, natural Send/Sync,
and a separately known-infallible `let Ok(...)`. Negative downstream checks keep
Rc-bearing captures/errors non-Send. There is no box, erased storage, new unsafe
code or extra capture allocation in the representation; allocation regression
testing and complete lifecycle integration have not been performed.

## Why it does not finish the port

1. A factory returning `impl Config` has a hidden concrete output type. Giving
   its metadata alias `impl Config` reintroduces TAIT (E0658); guessing the hidden
   implementation type is rejected (E0308). Stable generic storage does hold and
   resolve this output, but requires carrying the generic parameter in the helper
   signature. That does not preserve the current concrete-container contract.
2. An arbitrary function returning `Result<T, impl Error>` likewise does not
   give generated code a writable name for its hidden error. A placeholder in a
   metadata type alias is rejected (E0121). For known trait operations,
   `<u8 as FromStr>::Err` works; it does not generalize to arbitrary functions,
   inherent methods, closures or opaque results without more source information.
3. Knowing whether an initializer actually can fail remains separate from naming
   its error. The positive `fallibility_not_question_count` case contains `?` but
   returns actual never and permits `let Ok(...)`. The metadata fixture supplies
   a separate known-infallible build function; **automatic** fallibility selection
   for arbitrary declarations is not implemented or established by this probe.
   The existing declaration experiments already show that an opaque Error return
   loses the required irrefutability. Counting `?` would regress this example.

The unmodified workspace fails `cargo +stable check --workspace --offline --locked`
with E0554 at `#![feature(allow_internal_unstable)]` when bootstrap is removed.
Removing that permission gate alone would still leave generated TAIT uses.

## Migration consequence

A production stable port needs a decision about how otherwise unnameable types
enter the generated public items. Factory metadata is a possible new direction
for concrete-signature factories. It would require restrictions/new attributes
and further fallibility work; it is not a complete solution for existing inputs.
Generic storage/trait interfaces and boxed erasure remain alternatives with the
previously recorded public-API/allocation/auto-trait consequences, not approved
changes. Even solving closure storage does not remove declaration-error TAIT.

Preserving the entire current interface still needs an additional demonstrated
candidate. No reduction of supported captures, error inference or infallible
declarations was silently adopted in this investigation.

## Reproduce

From the Systasis package root:

```sh
env -u RUSTC_BOOTSTRAP rustc +stable --edition=2024 -Dwarnings -Funstable-features \
  research/stable-replacements/probes.rs -o /private/tmp/systasis-stable-replacements-driver
/private/tmp/systasis-stable-replacements-driver
```

The driver removes bootstrap for every compiler probe, forbids unstable features,
denies warnings, checks specific error codes on actual diagnostic lines, runs the
positive binaries and saves all probe sources/diagnostics in its printed temporary
directory. It uses no production macro crate or dependencies.

Verified with `rustc 1.100.0 (0ef1bc386 2026-11-09)`, aarch64 macOS. Latest artifacts:
`/var/folders/g9/s4vm_jd13rq47k_j05stjzf40000gq/T/systasis-stable-replacements-51832`.

Rust references checked:

- [Closure types](https://doc.rust-lang.org/reference/types/closure.html): closure
  types are anonymous; capture ownership does not supply a source-level name.
- [Impl trait](https://doc.rust-lang.org/reference/types/impl-trait.html): stable
  return-position opacity can carry unnamed types, but does not provide stable
  `impl Trait` field/type-alias syntax or reveal hidden concrete type equality.
