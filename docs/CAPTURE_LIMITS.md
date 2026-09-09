# Constructor capture implementation limits

The current generator stores captured bindings in typed fields. It identifies
those bindings before emitting the module-scope container type. Explicit binding
annotations remain the accepted source of capture types.

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
Struct-field types and aliases hiding a destructured shape remain unresolved.
See [capture-pattern regressions](../tests/capture_patterns.rs).
Explicit imports are preserved where their names do not conflict with capture
candidates and they do not depend on unhoisted function-local items. This includes
ordinary `use std::...` and function-local imports of `systasis_container`.
Glob imports and imports depending on function-local modules remain unsupported.
Cfg-controlled local bindings also need correction: analysis can select a type
annotation from a binding that rustc later removes. A discarded enclosing
function is a different case and does not demonstrate active-body cfg support.
These are implementation limitations, not changes to the agreed requirements.
