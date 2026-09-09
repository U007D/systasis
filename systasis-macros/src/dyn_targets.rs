//! Nominal trait-object targets for explicitly opted-in interface groups.

use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote};
use syn::{GenericParam, Generics, Lifetime, Type, parse_quote};

use crate::parse::InterfaceGroup;

pub(crate) struct DynTarget {
    /// Declare these items in the generated container module.
    pub(crate) declarations: TokenStream,
    /// The object type, relative to that same module, including its lifetime.
    pub(crate) ty: Type,
}

/// Preserve a single trait's target; combine larger groups into a nominal trait.
///
/// `generics` describe the enclosing configuration function. `object_lifetime`
/// is supplied by the caller so neither targets nor their borrowed accessors
/// accidentally acquire the default `'static` trait-object bound.
pub(crate) fn generate(
    index: usize,
    group: &InterfaceGroup,
    generics: &Generics,
    object_lifetime: &Lifetime,
) -> DynTarget {
    if group.0.len() == 1 {
        return DynTarget {
            declarations: TokenStream::new(),
            ty: parse_quote!(dyn #group + #object_lifetime),
        };
    }

    let name = format_ident!("__SystasisDyn{index}", span = Span::mixed_site());
    // Avoid colliding with an explicitly authored enclosing generic parameter.
    let value = value_parameter(generics);
    let (parameters, arguments, constraints) = generics.split_for_impl();
    let mut blanket_generics = generics.clone();
    blanket_generics.params.push(parse_quote!(#value: ?Sized));
    blanket_generics
        .make_where_clause()
        .predicates
        .push(parse_quote!(#value: #group));
    let (blanket_parameters, _, blanket_constraints) = blanket_generics.split_for_impl();

    DynTarget {
        declarations: quote! {
            pub trait #name #parameters: #group #constraints {}
            impl #blanket_parameters #name #arguments for #value #blanket_constraints {}
        },
        ty: parse_quote!(dyn #name #arguments + #object_lifetime),
    }
}

pub(crate) fn value_parameter(generics: &Generics) -> syn::Ident {
    (0..)
        .map(|suffix| format_ident!("__SystasisDynValue{suffix}", span = Span::mixed_site()))
        .find(|candidate| {
            !generics.params.iter().any(|parameter| match parameter {
                GenericParam::Type(parameter) => parameter.ident == *candidate,
                GenericParam::Const(parameter) => parameter.ident == *candidate,
                GenericParam::Lifetime(_) => false,
            })
        })
        .unwrap_or_else(|| {
            unreachable!("the finite parameter list cannot exhaust identifier suffixes")
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use quote::ToTokens;

    fn group(source: &str) -> InterfaceGroup {
        syn::parse_str(source).unwrap()
    }

    #[test]
    fn single_trait_retains_bindings_and_object_lifetime() {
        let target = generate(
            0,
            &group("IRead<'a, Item = &'a str>"),
            &Generics::default(),
            &parse_quote!('view),
        );
        assert!(target.declarations.is_empty());
        assert_eq!(
            target.ty.to_token_stream().to_string(),
            quote!(dyn IRead<'a, Item = &'a str> + 'view).to_string()
        );
    }

    #[test]
    fn combined_target_preserves_generic_arguments_bounds_and_associated_bindings() {
        let function: syn::ItemFn = parse_quote! {
            fn configure<'a, T: Clone, const N: usize>() where T: 'a {}
        };
        let target = generate(
            3,
            &group("IWrite<T, N> + IRead<'a, Item = &'a T>"),
            &function.sig.generics,
            &parse_quote!('view),
        );
        let items = syn::parse2::<syn::File>(target.declarations.clone()).unwrap();
        assert_eq!(items.items.len(), 2);
        assert_eq!(
            target.ty.to_token_stream().to_string(),
            quote!(dyn __SystasisDyn3<'a, T, N> + 'view).to_string()
        );
        let syn::Item::Trait(combined) = &items.items[0] else {
            panic!("expected combined trait");
        };
        assert_eq!(combined.generics.params.len(), 3);
        assert_eq!(combined.supertraits.len(), 2);
        assert_eq!(
            combined.generics.where_clause.to_token_stream().to_string(),
            quote!(where T: 'a).to_string()
        );
        let syn::Item::Impl(blanket) = &items.items[1] else {
            panic!("expected blanket implementation");
        };
        assert_eq!(blanket.generics.params.len(), 4);
        assert_eq!(
            blanket
                .generics
                .where_clause
                .as_ref()
                .unwrap()
                .predicates
                .len(),
            2
        );
    }

    #[test]
    fn blanket_parameter_avoids_existing_parameter_names() {
        let function: syn::ItemFn = parse_quote!(
            fn configure<__SystasisDynValue0>() {}
        );
        let target = generate(
            0,
            &group("IRead + IWrite"),
            &function.sig.generics,
            &parse_quote!('_),
        );
        assert!(
            target
                .declarations
                .to_string()
                .contains("__SystasisDynValue1")
        );
    }
}
