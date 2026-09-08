# systasis

A Rust dependency-injection library under development. Container construction
and registration macros are not implemented yet; this repository is not ready
for application use.

Implemented so far: checked std/Spin storage, shared/mutable guards, explicit
stored-value cloning, resolution errors, and the full `Fallible` conversions.
The storage support types are hidden implementation APIs, not a replacement
for the planned registration macros.

The root package is `systasis`; `systasis-macros/` is its procedural-macro
workspace member. The implementation follows the accepted design documents
in the parent directory. Research artifacts remain outside this package.

`std` is enabled by default. Disable default features for the no_std runtime.
The runtime can be checked on stable; the planned generated `Result<C, !>` API
currently needs the separately approved newer compiler. No additional nightly
guard feature is used.

## Validation

Run `cargo test --workspace` and repeat with `--no-default-features`. The Rust
compiler-test driver checks downstream rejection reasons, not just failed exits.
See [storage safety](docs/SAFETY.md) for invariants and [validation evidence](docs/VALIDATION.md)
for the tested scope. Physical-board behavior has not been validated.

## License

Licensed under either the MIT license or Apache License, Version 2.0, at your
option. See [LICENSE-MIT](LICENSE-MIT) and [LICENSE-APACHE](LICENSE-APACHE).
