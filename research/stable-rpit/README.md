# Stable inferred storage and build errors

**Parked on `stable-impl` at the user's request.** The generic public naming was
judged a usability regression from the nightly API. See [the branch decision](../README.md).
This archive does not authorize integration or further research.

Verified 2026-11-15 with `rustc 1.100.0 (0ef1bc386 2026-11-09)`.

This is a working stable representation for the two mechanisms currently using
TAIT: anonymous constructor storage and inferred declaration errors. It requires
generic public container/error types. It is a hand expansion with a small local
macro, not an implementation of Systasis's registration macro or a completed
stable port. The production macros, runtime and toolchain selection are unchanged.

## New result

Stable return-position `impl Trait` can preserve inferred source errors **and**
irrefutable `let Ok(container)`, provided the inferred error type occurs in the
successful container type as well. A zero-sized `PhantomData<fn() -> E>` carries
that type without making a successful container inherit the error's auto traits.

The public shape in each generated module is:

```rust,ignore
pub struct SystasisContainer<F = (), E = ()> {
    factory: F,
    error_type: PhantomData<fn() -> E>,
}

pub type SystasisContainerError<E> =
    Selected<E, { is_never(&kind) }>;
pub type BuildResult<F, E> =
    Result<SystasisContainer<F, E>, SystasisContainerError<E>>;

impl SystasisContainer {
    pub fn build() -> BuildResult<impl Fn() -> u8, impl Error + 'static> {
        finish(initialize!())
    }
}
```

`F` is the real native closure type; it is stored inline and called statically.
`E` is an inferred enum containing the original source errors inline. Neither
the caller nor the fixture's declaration supplies the error types or a factory
metadata attribute. A captured configuration returned as `impl IConfig` and an
initialization error returned as `impl Error` both work.

The default `()` parameters only provide the namespace for the associated
`SystasisContainer::build()` call. They do not name the resulting container type.

## Selecting the actual never type

The existing type-based `Infer<E>::kind()` mechanism selects `Impossible` for
actual never and `Possible<E>` for an inhabited error. `Kinds` combines each
source's associated `NEVER` constant. The generated helper returns that marker
using stable return-position `impl IErrorKind`.

The new bridge reads its constant through a function-item argument:

```rust
pub const fn is_never<F, K>(_: &F) -> bool
where
    F: FnOnce() -> K,
    K: IErrorKind,
{
    K::NEVER
}
```

`is_never(&kind)` never invokes `kind` or executes the registrations. It obtains
the marker's associated constant through generic inference. No source error type
is named in the generated public signature. A stable const-generic selector
normalizes the result error to actual never when every source error is never.
This is type-based selection, including when the initializer contains `?`; it
does not determine infallibility by counting propagation operators or measuring
layouts.

Two details matter:

1. Writing `impl Error` directly inside a qualified associated-type projection
   fails E0562. Put the projection inside an ordinary generic type alias and use
   `impl Error` as that alias's argument in the function return type.
2. If the infallible projection erases the only occurrence of the opaque error
   parameter, inference cannot define it (the initial minimal probe fell back to
   `()`). Retain the parameter in the successful container's zero-sized marker.

The selector's invariant diagnostic `unreachable!` follows the current production
mechanism: the kind helper and executed initializer use the same expression, so
the never branch receives no error value. No new unsafe mechanism is used.

## Verified behavior

The driver compiles a `#![no_std]` provider and independent downstream consumer
twice. Its `core` build has no `alloc` import or dependency. Its `alloc` build adds
user-created `Rc` values solely to check negative auto traits. The representation
adds no heap allocation, dynamic dispatch for constructor calls, unsafe code, or
dependency. `Error::source()` uses the ordinary borrowed standard error interface.

Runtime checks establish:

- `SystasisContainer::build()` infers the real anonymous factory type and source
  error types without caller annotations.
- Two infallible source propagations preserve downstream irrefutable `let Ok`.
- A downstream function can return `BuildResult<impl Fn() -> u8, impl Error>`;
  its caller also retains the infallible pattern.
- Hidden configuration and hidden error outputs need no factory metadata.
- Mixed parse and UTF-8 errors remain inline and downcast to their original
  source types through `Error::source()`.
- Owned capture construction happens once; fresh construction remains lazy and
  repeatable; state drops once; failed initialization drops prior state before
  returning the error. The uncalled marker helper has no runtime side effects.
- Ordinary captures/errors retain Send + Sync. A successful container does not
  inherit a non-Send initialization error's auto-trait restriction.
- External borrowed captures retain their lifetime.

Negative compiler checks verify actual diagnostic codes:

- Rc capture and Rc error cannot become Send (E0277).
- A possibly fallible result cannot use irrefutable `let Ok` (E0005).
- Borrowed capture cannot escape its owner (E0597).
- A bare `&SystasisContainer` parameter no longer accepts the inferred container
  (E0308), and a bare error alias needs its generic argument (E0107).

## API consequence and limits

Construction can retain its current spelling:

```rust,ignore
let Ok(container) = SystasisContainer::build();
```

Signatures which name the container need generics, for example the tested
downstream receiver:

```rust,ignore
fn receive<F, E>(container: &SystasisContainer<F, E>) -> u8
where
    F: Fn() -> u8,
{
    container.resolve_i_number()
}
```

The generated error name also becomes generic. This changes the accepted concrete
public naming contract; it has not been adopted. No unchanged-API stable solution
is established here. Unlike a trait-only container facade, the container remains
a struct and the demonstrated resolver returns its concrete registered type.

The probe covers one factory and two source error slots. Full registration
generation, zero-source declarations, multiple factories, arbitrary registration
types, named/nested subcontainers, exclusion metadata, generic configuration,
stored-value policies, and capture-lending integration are not implemented or
validated by it. The existing reconstructed-capture route is still needed where
constructors return borrows into their owned captures. This evidence supports a
specific stable design candidate, not a claim that the whole production API has
been ported or that every remaining integration issue is routine.

## Reproduce

From the repository root on `stable-impl`:

```sh
env -u RUSTC_BOOTSTRAP rustc +stable --edition=2024 -Dwarnings -Funstable-features \
  research/stable-rpit/driver.rs -o /private/tmp/systasis-stable-rpit-driver
/private/tmp/systasis-stable-rpit-driver /private/tmp/systasis-stable-rpit-results
```

The driver removes bootstrap for each compiler invocation, forbids unstable
features, denies warnings, records diagnostics, executes successful consumers,
and checks intended errors on negative cases. The optional argument selects the
evidence directory; without it, the driver creates a process-specific temporary
directory. Binaries and generated diagnostics are not committed to this archive.

The minimal mechanism is in [projection.rs](projection.rs). The runnable
[old API example](old-api-example.rs) uses the production crate on nightly;
the [proposed API example](new-api-example.rs) uses the research provider on stable.
[API-MINIMIZATION.md](API-MINIMIZATION.md) records the later signature checks.

The live requirements gist could not be retrieved in this environment (DNS
resolution failed). Current local requirements and recorded decisions were read;
no requirement was changed or treated as superseded.
