//! Container-generation macros for systasis, under development.
#![forbid(unsafe_code)]

mod analysis;
mod captures;
mod child;
mod child_codegen;
mod child_queries;
mod dyn_targets;
mod generate;
mod generic_policy;
mod graph;
mod parse;
#[cfg(test)]
mod parse_properties;
mod rebase;
mod requirements;
mod scopegen;
mod wiring;

/// Generate the module-scope `AppContainer` declared inside this function.
///
/// The function contains one `systasis_container!` declaration with stored values,
/// fresh constructors, or named child containers. Its consuming `.build()`
/// transition returns `Result<&AppContainer, E>`; generated code retains the
/// pinned owner until the enclosing scope exits.
///
/// Optional `require(Send)`, `require(Sync)`, or `require(Send, Sync)` arguments
/// assert the completed container's auto traits. `require(!Sync)` instead selects
/// local, non-atomic borrow tracking for mutable/takeable values; it may be
/// combined with `Send`, but not `Sync`.
#[proc_macro_attribute]
pub fn container(
    args: proc_macro::TokenStream,
    input: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    let requirements = syn::parse_macro_input!(args as requirements::Requirements);
    generate::expand(
        syn::parse_macro_input!(input as syn::ItemFn),
        requirements.local,
        &requirements.positive,
    )
    .unwrap_or_else(syn::Error::into_compile_error)
    .into()
}

/// Declare registrations inside a `#[systasis::container]` function.
///
/// Use `register_value!(expression: Type as Interface)` for a stored value,
/// `register_type!(Type as Interface)` for a fresh `Default` value, and
/// `register_type_with!(Type as Interface, move || expression)` for a repeatable
/// constructor. Captured local bindings need explicit type annotations.
/// `register_container!(name: &ChildType)` composes an independently owned child
/// behind a named scope rather than importing its registrations into the parent.
///
/// The enclosing attribute processes this declaration and its dependency queries
/// together. Invoke `.build()` before using the generated resolution methods.
#[proc_macro]
pub fn systasis_container(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    if input.to_string() == "@ __systasis_marker" {
        return quote::quote!(()).into();
    }
    syn::Error::new(
        proc_macro2::Span::call_site(),
        "place systasis_container! inside a #[systasis::container] function and call .build()",
    )
    .into_compile_error()
    .into()
}
