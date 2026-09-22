//! Container-generation macros for systasis, under development.
#![forbid(unsafe_code)]
#![feature(allow_internal_unstable)]
#![allow(internal_features)]

mod analysis;
mod captures;
mod child;
mod child_codegen;
mod child_queries;
mod configuration;
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

/// Internal configuration selector; invoked only by generated helper items.
#[doc(hidden)]
#[proc_macro_derive(
    __SystasisSelectConfiguration,
    attributes(__systasis_configuration_source, __systasis_configuration_args)
)]
#[allow_internal_unstable(type_alias_impl_trait)]
pub fn select_configuration(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    configuration::resume(syn::parse_macro_input!(input as syn::ItemStruct))
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

/// Removes the helper item after its preceding derive has emitted the function.
#[doc(hidden)]
#[proc_macro_attribute]
pub fn __systasis_erase_configuration(
    _: proc_macro::TokenStream,
    _: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    proc_macro::TokenStream::new()
}

/// Generate the module-scope `AppContainer` declared inside this function.
///
/// The function contains one `systasis_container!` declaration with stored values,
/// fresh constructors, or named child containers. Its consuming `.build()`
/// transition returns `Result<AppContainer, E>`, transferring ownership to the
/// caller. Resolved references and guards borrow that owned container.
///
/// Optional `require(Send)`, `require(Sync)`, or `require(Send, Sync)` arguments
/// assert the completed container's auto traits. `require(!Sync)` instead selects
/// local, non-atomic borrow tracking for mutable/takeable values; it may be
/// combined with `Send`, but not `Sync`.
#[proc_macro_attribute]
#[allow_internal_unstable(type_alias_impl_trait)]
pub fn container(
    args: proc_macro::TokenStream,
    input: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    let arguments: proc_macro2::TokenStream = args.clone().into();
    let requirements = syn::parse_macro_input!(args as requirements::Requirements);
    let function = syn::parse_macro_input!(input as syn::ItemFn);
    configuration::select(&function, &arguments)
        .and_then(|selected| {
            selected.map_or_else(
                || generate::expand(function, requirements.local, &requirements.positive),
                Ok,
            )
        })
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

/// Declare registrations inside a `#[systasis::container]` function.
///
/// Use `register_value!(expression: Type as Interface)` for a stored value,
/// `register_type!(Type as Interface)` for a fresh `Default` value, and
/// `register_type_with!(Type as Interface, move || expression)` for a repeatable
/// constructor. Show captured bindings with explicit types; annotations may be
/// omitted where Rust can infer their types.
/// `register_container!(name: &ChildType)` composes an independently owned child
/// behind a named scope rather than importing its registrations into the parent.
///
/// The enclosing attribute processes this declaration and its dependency queries
/// together. The builder can be moved or dropped without running initializers;
/// consuming `.build()` exposes the generated resolution methods.
///
/// Systasis cannot yet represent some borrowed constructor inputs' lifetimes.
/// Workaround: register an implementation with owned fields and captures.
/// Not all affected compiler errors include this guidance.
#[proc_macro]
pub fn systasis_container(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    if input.to_string() == "@ __systasis_marker" {
        return quote::quote!(()).into();
    }
    syn::Error::new(
        proc_macro2::Span::call_site(),
        "bind systasis_container! to a local inside a #[systasis::container] function",
    )
    .into_compile_error()
    .into()
}
