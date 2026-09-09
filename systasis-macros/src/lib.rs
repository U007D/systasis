//! Container-generation macros for systasis, under development.
#![forbid(unsafe_code)]

mod analysis;
mod generate;
mod graph;
mod parse;
mod requirements;

/// Generate a stored-value container declared inside this function.
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

/// A container declaration must be processed by the enclosing attribute.
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
