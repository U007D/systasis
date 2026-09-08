# systasis

A Rust dependency-injection library under development. Container construction
and registration macros are not implemented yet; this repository is not ready
for application use.

The root package is `systasis`; `systasis-macros/` is its procedural-macro
workspace member. The implementation follows the accepted design documents
in the parent directory. Research artifacts remain outside this package.

`std` is enabled by default. Disable default features for the no_std runtime.
The runtime can be checked on stable; the planned generated `Result<C, !>` API
currently needs the separately approved newer compiler. No additional nightly
guard feature is used.

## License

Licensed under either the MIT license or Apache License, Version 2.0, at your
option. See [LICENSE-MIT](LICENSE-MIT) and [LICENSE-APACHE](LICENSE-APACHE).
