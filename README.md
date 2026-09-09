# systasis

A Rust dependency-injection library under development. Basic container generation
is implemented; the complete requirements are not yet ready for application use.

Implemented so far: checked std/no_std storage, shared/mutable guards, explicit
stored-value cloning, resolution errors, and the full `Fallible` conversions.
The storage support types are hidden implementation APIs, not a replacement
for the generated registration macros.

The generated path supports stored values, fresh Default constructors, owned
dependency injection, concrete registered-type lookup, overrides, dependency
layers, checked access, cloning, single-trait dyn access, and optional Send/Sync
requirements or local !Sync storage. See the runnable [owned-dependency example](examples/owned.rs).
Services and their constructors remain ordinary generic Rust; dependencies are
transferred by value, without hidden wrappers or field rewriting.

Still pending: captured custom constructors, generic enclosing functions,
interface groups, named namespaces and composition, unchecked generation,
renamed Cargo dependencies, and embedded validation. Container-stored services
retaining internal borrows are deferred. These are not removed requirements.

The root package is `systasis`; `systasis-macros/` is its procedural-macro
workspace member. The implementation follows the accepted design documents
in the parent directory. Research artifacts remain outside this package.

`std` is enabled by default. Disable default features for the no_std runtime.
The current generated path and runtime pass stable Rust tests. Infallible builds
infer the never error type without spelling `!` in generated source. Explicit
`.build::<!>()` still depends on the caller's compiler accepting that type syntax.
No nightly guard feature is used.

## Validation

Run `cargo test --workspace` and repeat with `--no-default-features`. The Rust
compiler-test driver checks downstream rejection reasons, not just failed exits.
See [storage safety](docs/SAFETY.md) for invariants and [validation evidence](docs/VALIDATION.md)
for the tested scope. Physical-board behavior has not been validated.

## License

Licensed under either the MIT license or Apache License, Version 2.0, at your
option. See [LICENSE-MIT](LICENSE-MIT) and [LICENSE-APACHE](LICENSE-APACHE).
