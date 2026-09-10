# Constructor capture implementation limits

The current generator stores captured bindings in typed fields. It identifies
those bindings before emitting the module-scope container type. Explicit binding
annotations remain the accepted source of capture types.

## Reserved generated names on stable

A glob import inside a value initializer can shadow a generated storage or child
identifier. A compatible caller-owned mock can then supply the resolver result
instead of the registered value. The constructor-context fix does not protect
this path. For now, `__systasis_*` names are reserved for generated code: caller
declarations, imports (including globs), and macro expansions must not introduce
them into generated-code scopes. Ordinary nonconflicting globs remain allowed.
This accepted usage restriction keeps the stable implementation; it does not
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

Capture analysis currently rejects unexpanded macros inside custom constructors.
Their input tokens do not necessarily identify the values their expansion reads:
tokens may be a macro-specific language, and formatting can refer to a binding
inside a string literal. Replacing identifier tokens as if they were Rust
expressions would not preserve arbitrary macro semantics.

The compiler's eager macro expansion is not generally available to user macros.
The experimental `TokenStream::expand_expr` API is nightly-only and currently
accepts expressions expanding to literals, not arbitrary constructor bodies.
See the [compiler expansion guide](https://rustc-dev-guide.rust-lang.org/macro-expansion.html)
and [proc_macro API](https://doc.rust-lang.org/proc_macro/struct.TokenStream.html#method.expand_expr).
This rules out that particular expansion mechanism; it is not a proof that all
possible implementations of the desired inline syntax are impossible.

A source-local-method candidate preserved noncapturing macros in tested cases,
but was not adopted: an inner macro declaration could bypass its hidden-query
rejection and invoke caller code instead of resolving the registered value.
Typed outer captures through macros also remain unimplemented. Production
continues to reject opaque constructors; the failed candidate is not proof
that a correct stable implementation is impossible.

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
Struct-field types and tuple aliases with unknown-arity rest patterns remain
unresolved. See [capture-pattern regressions](../tests/capture_patterns.rs).
Explicit imports are preserved where their names do not conflict with capture
candidates and they do not depend on unhoisted function-local items. This includes
ordinary `use std::...` and function-local imports of `systasis_container`.
Closure-local glob imports remain with their original body. They are supported
where no referenced outer binding could be shadowed by that glob. A glob in one
block does not disable captures in a sibling block. Generated constructor queries
use private context fields and anchored helper paths, so imported names cannot
replace their local or child storage. Ambiguous outer references still produce
an implementation-limit diagnostic; the macro does not inspect a glob's exports.
See [glob regressions](../tests/capture_globs.rs).
Enclosing-function globs and imports depending on function-local modules remain
unsupported.
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
