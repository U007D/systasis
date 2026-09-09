# systasis

A Rust dependency-injection library under development. Basic container generation
is implemented; the complete requirements are not yet ready for application use.

Implemented so far: checked std/no_std storage, shared/mutable guards, explicit
stored-value cloning, resolution errors, and the full `Fallible` conversions.
The storage support types are hidden implementation APIs, not a replacement
for the generated registration macros.

The generated path supports stored values, fresh Default and custom constructors, owned
dependency injection, concrete registered-type lookup, overrides, dependency
layers, checked access, cloning, multi-trait groups, local namespaces, explicit dyn access, and optional Send/Sync
requirements or local !Sync storage. See the runnable [owned-dependency example](examples/owned.rs).
The [usage guide](docs/USAGE.md) is also the crate-level API documentation;
its examples compile and run as doctests.
The requirements' complete [quick example](examples/quick_start.rs) is also
runnable, with its implementation types kept private.
Services and their constructors remain ordinary generic Rust; dependencies are
transferred by value, without hidden wrappers or field rewriting.

Custom constructors own explicitly typed captured bindings and run on resolution.
Fallible constructors preserve their annotated return type. Returned values may
borrow captures or retain dependency guards; storing such borrowed results inside
the container remains deferred. See [constructor tests](tests/custom_constructor.rs).
Capture analysis supports explicitly typed tuple/array destructuring, exact-arity
tuple aliases, explicit and implicit reference bindings, and elided reference
lifetimes in typed function parameters.
Explicit imports are preserved when they do not conflict with
capture names or depend on unhoisted function-local items. Opaque macros, glob imports,
struct destructuring, unknown-arity tuple-alias rest patterns, and some
cfg-controlled capture cases remain
implementation gaps, not new API rules. See [capture limits](docs/CAPTURE_LIMITS.md).
Array/slice rest captures support literal lengths, simple concrete const paths
and arithmetic, and borrowed slices.

Generic enclosing functions preserve authored type, const and lifetime parameters
in `AppContainer`, with registration-site Copy policy. See
[generic tests](tests/generic_container.rs) and [cross-crate checks](tests/generic_cross_crate.rs).
Source-relative paths are preserved when hoisted. Differently spelled equivalent
Copy bounds remain a recognition gap.

Local namespace queries use the documented `_from` forms and public methods use
`_in_namespace` suffixes; omitted and explicit `default` select the same registrations.
See [namespace tests](tests/namespaces.rs).

Named child containers use `register_container!(primary: &ChildAlias)`.
`container.primary()` returns a reference to `primary::SubContainer`, with the
child's ownership exclusions retained. Nested accessors and `_from` queries
follow child paths, for example `try_resolve_ref_from!(IValue, branch::primary)`.
Children are built and owned independently; composition does not transfer them.
See [nested examples](tests/nested_children.rs) and [child namespaces](tests/child_namespaces.rs).
The runnable [scope-injection example](examples/scopes.rs) passes a named child
scope to a function without a resolver callback or the outer container.
Container aliases and import renames work. Resolver arguments identify registrations
only inside the selected container or subcontainer. No trait import is required
for a child query; unrelated surrounding names do not affect it. A missing
registration is an error, with no fallback to an outside trait or alias.
Registration type/interface annotations still refer to ordinary Rust types/traits.

The default-off `resolve_unchecked` feature adds unsafe nonblocking accessors
for consumable values. Guards and ownership exclusions are preserved; see
[safety contracts](docs/SAFETY.md) and [examples under test](tests/unchecked.rs).

Allocation regression tests cover nonallocating construction, resolution,
cloning, nested scopes, failed-build cleanup and owner destruction. They measure
specified workloads, not arbitrary caller constructors or Clone implementations.
See [the test driver](tests/allocations.rs) and [evidence](docs/VALIDATION.md).

Still pending: remaining capture cases, documentation/example completeness,
and hardware validation. Container-stored services retaining internal borrows are
deferred. Renamed Cargo dependency support is out of the current scope; no import
placement restriction or new dependency has been adopted for it.

The root package is `systasis`; `systasis-macros/` is its procedural-macro
workspace member. The implementation follows the accepted design documents
in the parent directory. Research artifacts remain outside this package.

`std` is enabled by default. Disable default features for the no_std runtime.
The current generated path and runtime pass stable Rust tests. Infallible builds
infer the never error type without spelling `!` in generated source. Explicit
`.build::<!>()` still depends on the caller's compiler accepting that type syntax.
No nightly guard feature is used.

For targets without native atomic compare-and-swap, enable `portable-atomic`
alongside `default-features = false`. It enables Spin's portable atomics and the
critical-section fallback; the application must supply a platform-appropriate
`critical-section` implementation. Systasis does not assume a single core or
install an interrupt handler. `experimental-hardware` enables this integration
and its embedded compile/link checks; physical-board validation remains deferred.

## Validation

Run `cargo test --workspace` and repeat with `--no-default-features`. The Rust
compiler-test driver checks downstream rejection reasons, not just failed exits.
See [storage safety](docs/SAFETY.md) for invariants and [validation evidence](docs/VALIDATION.md)
for the tested scope. Physical-board behavior has not been validated.
Local extracted-package consumers are checked separately with
`cargo test --test packaged_consumer --offline --locked -- --ignored --nocapture`;
this does not publish either crate.

## License

Licensed under either the MIT license or Apache License, Version 2.0, at your
option. See [LICENSE-MIT](LICENSE-MIT) and [LICENSE-APACHE](LICENSE-APACHE).
