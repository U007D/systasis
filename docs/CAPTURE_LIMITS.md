# Constructor capture implementation limits

The generator retains reconstructed typed captures where source analysis succeeds
and otherwise uses Rust-native closure storage, including macro-containing bodies. Explicit
annotations on captured bindings remain part of the documented syntax. They may
be omitted wherever Rust can infer their types; native storage supports this
without changing ownership or auto traits. Reconstruction still needs enough
source type information to name its capture fields. A failed reconstruction
restarts from the original closure body, without retaining partial rewrites.
See [fallback regressions](../tests/native_fallback.rs) for inferred owned
bindings (including authored generics), struct/tuple-struct patterns, tuple-alias
rest patterns, macro-created bindings and enclosing-function glob imports.

## Reserved generated names on stable

A glob import inside a value initializer can shadow a generated storage or child
identifier. A compatible caller-owned mock can then supply the resolver result
instead of the registered value. The constructor-context fix does not protect
this path. For now, `__systasis_*` names are reserved for generated code: caller
declarations, imports (including globs), and macro expansions must not introduce
them into generated-code scopes. Ordinary nonconflicting globs remain allowed.
This accepted usage restriction remains during the nightly port; it does not
fix the collision or guarantee a compiler diagnostic. A violating program can
compile and resolve the wrong value. The parent research/initializer-hygiene
directory retains the reproducer and rejected stable candidates. The nightly
definition-site candidate has not been adopted.

Rust compiler errors may expose generated types and paths rather than their
public aliases. For example, an incorrect resolver call can mention
`Generated<systasis::__private::TakeSlot<String>, (), fn() -> (String,)>`
instead of `AppContainer`. These are implementation names, not types callers
must write in signatures; exact diagnostic text is compiler-dependent.

## Macros inside the constructor

Macro-containing constructors now remain native `move` closures in their original
source scope. Rust determines their captures after expanding the body. Macro
input tokens do not necessarily identify the values their expansion reads:
tokens may be a macro-specific language, and formatting can refer to a binding
inside a string literal. Replacing identifier tokens as if they were Rust
expressions would not preserve arbitrary macro semantics. For example, a typed
`config: String` used by `format!("{config}")` is owned only by that constructor;
it is not a registration. The generated container keeps its concrete name.

[Native constructor tests](../tests/native_constructor.rs) cover lazy/repeated
calls, source-local macros, failure cleanup, generic captures with explicit
external lifetimes, direct dependency queries, returned guards and multiple
subcontainers. [Compiler tests](../tests/native_compiler.rs) check cross-crate
naming, private result types, scoped forwarding and ownership/auto-trait errors.
Application code does not need feature annotations; see [NIGHTLY.md](NIGHTLY.md).

[Child-context tests](../tests/native_child_context.rs) also cover elided outer
child-reference lifetimes, private child payloads borrowing external data,
independent payload lifetimes across two children, nested children with owned
payloads, and native-to-reconstructed constructor calls returning a child guard.
Returned child write guards retain the child's synchronized or local policy.
The context stores restricted backing borrows, not references to temporary scope
descriptors. Direct queries rebuild only their selected child's descriptor.
[Generic child tests](../tests/native_child_generics.rs) preserve private concrete
payloads, explicit backing-lifetime outputs and chains of both constructor forms.

This is not yet arbitrary macro-body support. Queries introduced only by a later
macro expansion still need graph/exclusion integration. Nongeneric functions
capturing locally borrowed inputs, enclosing argument-position `impl Trait`
parameters, nested children whose payloads borrow external data, and other
synthesized dependency lifetimes still have native-storage limitations.
Returning a borrow into an owned native capture also remains
unsupported; existing non-macro constructors retain their previous returned-borrow
implementation. These are implementation gaps, not new accepted restrictions.

The compiler's eager macro expansion is not generally available to user macros.
The experimental `TokenStream::expand_expr` API is nightly-only and currently
accepts expressions expanding to literals, not arbitrary constructor bodies.
See the [compiler expansion guide](https://rustc-dev-guide.rust-lang.org/macro-expansion.html)
and [proc_macro API](https://doc.rust-lang.org/proc_macro/struct.TokenStream.html#method.expand_expr).
This rules out that particular expansion mechanism; it is not a proof that all
possible implementations of the desired inline syntax are impossible.

A source-local-method candidate preserved noncapturing macros in tested cases,
but remains outside production. Its earlier rejection report overstated one
counterexample: a caller-defined macro named `try_resolve` discarded its argument
and returned caller data. That demonstrates ordinary macro shadowing, not an
actual systasis lookup falling back to an outside registration. Genuine queries
introduced by macro expansion still need dependency-analysis and exclusion
checks. Its typed outer captures were not solved. The native path above supersedes
blanket rejection of macro-containing constructors, without establishing full
macro/query integration or proving stable alternatives impossible.

Moving the macro into an ordinary function called by the constructor works:
see [the tested example](../tests/constructor_macro_helper.rs). Both host backends
verify zero calls at build and one call per resolution. This is a workaround,
not an adopted requirement to add helper functions. Precomputing the result before
the container would change execution timing and is not an equivalent solution.

Do not resolve this gap by silently capturing extra bindings, changing captured
values into references, adding allocation/type erasure, or adding new annotations.
Those alternatives would need separate evaluation and user approval.

## Other remaining gaps

Tuple/array annotations and explicit reference patterns can supply individual
binding types. Implicit reference bindings from structurally annotated tuples
and arrays are also supported, including nested shared/mutable reference layers.
Their original patterns remain in place for rustc's edition checks. Array/slice
rest bindings are supported for literal lengths, simple concrete const paths and
arithmetic, and borrowed slices. Generic-dependent remainder expressions are not
synthesized; other const-expression shapes still need work. See
[rest-capture regressions](../tests/capture_rest.rs).
Exact-arity tuple aliases are supported, including nested reference layers and
private capture types. Generated capture records retain ordinary typed fields;
no extra generic parameter or wrapper is required in caller code. See
[tuple-alias regressions](../tests/capture_tuple_aliases.rs).
Sequence projections also support generic array elements, concrete owned/borrowed
array-alias remainders, and shared/mutable slice-alias remainders.
They retain the exact array or slice type rather than coercing arrays to slices.
Generic-argument array remainder aliases remain unresolved in this implementation. See
[sequence-alias regressions](../tests/capture_sequence_aliases.rs).
Reconstruction cannot determine struct-field types or tuple aliases with
unknown-arity rest patterns. Native fallback handles the tested struct,
tuple-struct and tuple-alias rest captures without naming those field types in
generated signatures.
See [capture-pattern regressions](../tests/capture_patterns.rs) and the fallback
tests above; remaining native lifetime limits still apply.
Explicit imports are preserved where their names do not conflict with capture
candidates and they do not depend on unhoisted function-local items. This includes
ordinary `use std::...` and function-local imports of `systasis_container`.
Closure-local glob imports remain with their original body. They are supported
where no referenced outer binding could be shadowed by that glob. A glob in one
block does not disable captures in a sibling block. Generated constructor queries
use private context fields and anchored helper paths, so imported names cannot
replace their local or child storage. Ambiguous outer references now select
native capture handling; the macro does not inspect a glob's exports.
See [glob regressions](../tests/capture_globs.rs).
Enclosing-function globs also select native handling. Imports depending on local
modules follow that path rather than being hoisted into generated storage;
their interaction with native lifetime limits still needs verification.
Local `cfg` and selection-producing `cfg_attr` attributes are selected by rustc
before capture analysis. Conditional statement ancestors are selected before
their contents, so a disabled block does not expose its nested predicates.
Tests cover conflicting binding annotations, nested ordinary blocks, and
constructor ownership on both backends. Other conditional expressions, match
arms and struct-literal fields remain rustc-owned: selection does not descend
through their conditional boundaries. Opaque macro bodies, signature and
registration configuration still require separate handling; this is not general
cfg support. A temporary helper lets rustc select all conditions before capture
analysis; expansion depth does not increase per selected statement. The helper
is erased after emitting the function and module-scope container. See
[configured-capture regressions](../tests/configured_captures.rs).
These are implementation limitations, not changes to the agreed requirements.
