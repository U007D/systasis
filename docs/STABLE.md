# Stable Rust support

`stable-impl` supports Rust 1.100 and newer. The repository selects `stable`;
both workspace crates forbid unstable features. No bootstrap environment setting,
compiler wrapper, or application feature attribute is needed.

Registration syntax and resolution behavior are unchanged. Containers own their
values and constructor state inline, use static dispatch, and retain their natural
auto traits. The std and allocator-free no_std runtime configurations share this
representation. Existing capture limitations remain in [CAPTURE_LIMITS.md](CAPTURE_LIMITS.md).

## Naming inferred types

Stable Rust represents anonymous native closures and initialization errors through
generic parameters. Functions naming these containers need the corresponding
parameters. Construction at an inferred call site keeps its spelling:

```rust,ignore
let Ok(container) = SystasisContainer::build();
```

- Ordinary value-only declarations without error propagation or unexpanded initializer macros retain the bare
  `SystasisContainer` type.
- Native constructor storage adds `F`, bounded by the generated `SystasisFactories`
  trait. One parameter represents all native constructors, including dependencies
  between constructors. Their concrete closure types remain inferred.
- Inferred initialization errors add `E`. The generated `BuildResult<E>` aliases
  `Result<SystasisContainer<E>, SystasisContainerError<E>>`.
- When both are needed, use `SystasisContainer<F, E>` and `BuildResult<F, E>`.
  Authored lifetime, type, and const parameters precede these added parameters.

The default parameters provide the `SystasisContainer::build()` entry point.
They do not name an arbitrary built container's inferred types.
An initializer macro can also introduce `E`, since its expansion may return an
error. The compiler still determines whether that build is infallible.

For example, a function returning a native-constructor container uses:

```rust,ignore
fn make(label: String) -> SystasisContainer<impl SystasisFactories> { /* ... */ }

fn read<F>(container: &SystasisContainer<F>) -> String
where
    F: SystasisFactories,
{
    container.resolve_i_label()
}
```

Named child containers and `child::SubContainer` aliases carry the relevant parent
type parameters in the same way. A parent taking `&child::SystasisContainer<F>`
uses the bound `F: child::SystasisFactories`; nested scope access and exclusions
retain their existing behavior. Prefer returning a built container when a former
callback signature would need to name its anonymous constructor types.

The [usage guide](USAGE.md#naming-native-constructor-storage) has a complete,
compiled example. Cross-crate construction, named scopes, private output types,
and external lifetimes are exercised by `tests/native_compiler.rs`.

## Inferred errors

For a declaration with inferred sources, a returning function can use:

```rust,ignore
fn make() -> BuildResult<impl core::error::Error> {
    SystasisContainer::build()
}
```

Add `impl SystasisFactories` as the first argument when native storage is present.
`BuildResult` is a standard `Result` alias, sharing one inferred `E` between the
successful container and its error. Source errors remain inline, and
`Error::source()` exposes the original error. The successful container does not
inherit a source error's `Send`/`Sync` restrictions.

Infallibility is selected from the compiler-inferred source error types. When
every source is the actual never type, the result error is also never, preserving
irrefutable `let Ok(container)`, including through a `BuildResult` return signature.
The type-checking helper used for selection never runs initializers.

## Verification

```sh
env -u RUSTC_BOOTSTRAP RUSTFLAGS=-Funstable-features cargo +stable test --workspace --offline --locked --features resolve_unchecked,portable-atomic
env -u RUSTC_BOOTSTRAP RUSTFLAGS=-Funstable-features cargo +stable test --workspace --offline --locked --no-default-features --features resolve_unchecked,portable-atomic
```

The compiler-fixture driver also removes bootstrap and forbids unstable features
for downstream consumers. See [VALIDATION.md](VALIDATION.md) for recorded results.
The original hand-expanded probes remain in `research/stable-rpit/` as historical
evidence; production support is implemented in the macro crate.
