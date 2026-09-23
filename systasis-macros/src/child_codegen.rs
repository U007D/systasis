//! Forward nested metadata and operations through stored restricted descriptors.

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{Ident, Index, ext::IdentExt};

/// Keep child scope projections behind a nominal type's well-formedness boundary.
pub(crate) fn stored_children(
    generics: &syn::Generics,
    tuple: &TokenStream,
    marker: &TokenStream,
    identity: &TokenStream,
) -> TokenStream {
    let original_generics = generics;
    let identity_arguments = generics.params.iter().map(|parameter| match parameter {
        syn::GenericParam::Type(parameter) => {
            let name = &parameter.ident;
            quote!(#name)
        }
        syn::GenericParam::Lifetime(parameter) => {
            let name = &parameter.lifetime;
            quote!(#name)
        }
        syn::GenericParam::Const(parameter) => {
            let name = &parameter.ident;
            quote!(#name)
        }
    });
    let target = quote!(__systasis_StoredChildren<#(#identity_arguments,)* #identity>);
    // The identity preserves private payload spelling in the implementing type.
    // The separate direct marker makes otherwise unrelated generics used.
    // New parameters use the reserved prefix because authored parameters are
    // copied into these same scopes (including the associated-type lifetime).
    let mut generics = generics.clone();
    generics
        .params
        .push(syn::parse_quote!(__systasis_stored_identity));
    let (parameters, _, predicates) = generics.split_for_impl();
    let mut emitted = vec![quote!(
        pub struct __systasis_StoredChildren #parameters #predicates {
            pub(super) inner: #tuple,
            pub(super) marker: ::core::marker::PhantomData<(#marker, fn() -> __systasis_stored_identity)>,
        }
    )];
    for (name, parameters, associated, gat) in [
        (
            "__systasis_ChildRegistrationPolicy",
            quote!(__systasis_child_key, __systasis_rest, __systasis_key, const __systasis_LOCAL: bool),
            "Policy",
            false,
        ),
        (
            "__systasis_ChildBorrowed",
            quote!(__systasis_child_key, __systasis_rest, __systasis_key),
            "Mask",
            false,
        ),
        (
            "__systasis_ChildRegistered",
            quote!(__systasis_child_key, __systasis_rest, __systasis_key),
            "Value",
            true,
        ),
        (
            "__systasis_ChildDynRegistered",
            quote!(__systasis_child_key, __systasis_rest, __systasis_key),
            "Target",
            true,
        ),
        (
            "__systasis_ChildOutput",
            quote!(
                __systasis_child_key,
                __systasis_rest,
                __systasis_key,
                __systasis_operation
            ),
            "Value",
            true,
        ),
        (
            "__systasis_ScopeChildren",
            quote!(__systasis_restrictions),
            "Output",
            false,
        ),
    ] {
        let name = format_ident!("{name}");
        let associated = format_ident!("{associated}");
        let extra: syn::Generics = syn::parse2(quote!(<#parameters>)).unwrap_or_else(|_| {
            unreachable!("the fixed child metadata parameter lists are valid Rust generics")
        });
        let (_, extra_arguments, _) = extra.split_for_impl();
        let trait_type = quote!(#name #extra_arguments);
        let mut implementation = original_generics.clone();
        implementation.params.extend(extra.params.iter().cloned());
        implementation
            .make_where_clause()
            .predicates
            .push(syn::parse_quote!(#tuple: #trait_type));
        let (impl_parameters, _, impl_predicates) = implementation.split_for_impl();
        let definition = if gat {
            quote!(type #associated<'__systasis_item> = <#tuple as #trait_type>::#associated<'__systasis_item> where Self: '__systasis_item;)
        } else {
            quote!(type #associated = <#tuple as #trait_type>::#associated;)
        };
        let method = (name == "__systasis_ScopeChildren").then(|| quote!(
            fn restricted(&self) -> Self::Output { <#tuple as #trait_type>::restricted(&self.inner) }
        ));
        emitted.push(quote!(
            impl #impl_parameters #trait_type for #target #impl_predicates {
                #definition
                #method
            }
        ));
    }
    quote!(#(#emitted)*)
}

/// A namespace at the end of a child path selects local registrations, not a child.
pub(crate) fn namespaces(
    parameters: &[Ident],
    registrations: &[crate::parse::Registration],
) -> TokenStream {
    let names = registrations
        .iter()
        .map(|r| {
            r.namespace
                .0
                .as_ref()
                .map_or_else(|| "default".to_owned(), |name| name.unraw().to_string())
        })
        .collect::<std::collections::BTreeSet<_>>();
    let mut emitted = Vec::new();
    for name in names {
        let key = crate::scopegen::key(&name);
        let here = if name == "default" {
            quote!(::systasis::scoped::Here)
        } else {
            quote!(::systasis::scoped::Here<#key>)
        };
        let there = quote!(::systasis::scoped::There<#key, ::systasis::scoped::Here>);
        for (public, associated, operation) in [
            ("Registered", "Value", false),
            ("DynRegistered", "Target", false),
            ("Output", "Value", true),
        ] {
            let public = format_ident!("{public}");
            let associated = format_ident!("{associated}");
            let operation = operation.then(|| quote!(, __Operation));
            emitted.push(quote!(
                impl<#(#parameters,)* __Key #operation> ::systasis::scoped::#public<#there, __Key #operation>
                    for __systasis_Generated<#(#parameters),*>
                where Self: ::systasis::scoped::#public<#here, __Key #operation> {
                    type #associated<'a> = <Self as ::systasis::scoped::#public<#here, __Key #operation>>::#associated<'a> where Self: 'a;
                }
            ));
        }
        emitted.push(quote!(
            impl<#(#parameters,)* __Key, const __LOCAL: bool>
                ::systasis::scoped::RegistrationPolicy<#there, __Key, __LOCAL> for __systasis_Generated<#(#parameters),*>
            where Self: ::systasis::scoped::RegistrationPolicy<#here, __Key, __LOCAL> {
                type Policy = <Self as ::systasis::scoped::RegistrationPolicy<#here, __Key, __LOCAL>>::Policy;
            }
            impl<#(#parameters,)* __Key> ::systasis::scoped::Borrowed<#there, __Key> for __systasis_Generated<#(#parameters),*>
            where Self: ::systasis::scoped::Borrowed<#here, __Key> {
                type Mask = <Self as ::systasis::scoped::Borrowed<#here, __Key>>::Mask;
            }
        ));
        let mut operations = vec![("Resolve", false)];
        if cfg!(feature = "resolve_unchecked") {
            operations.push(("UnsafeResolve", true));
        }
        for (operation, unchecked) in operations {
            let operation = format_ident!("{operation}");
            let safety = unchecked.then(|| quote!(unsafe));
            let call = quote!(<Self as ::systasis::scoped::#operation<'__backing, #here, __Key, __Operation>>::resolve(self));
            let call = if unchecked {
                quote!({
                    // SAFETY: changing only the path spelling preserves the selected
                    // operation, its acquisition preconditions, and its exclusions.
                    unsafe { #call }
                })
            } else {
                call
            };
            emitted.push(quote!(
                impl<'__backing, __Container: ?Sized, __Restrictions, __Children, __Key, __Operation>
                    ::systasis::scoped::#operation<'__backing, #there, __Key, __Operation>
                    for __systasis_Scope<'__backing, __Container, __Restrictions, __Children>
                where Self: ::systasis::scoped::#operation<'__backing, #here, __Key, __Operation> {
                    type Output = <Self as ::systasis::scoped::#operation<'__backing, #here, __Key, __Operation>>::Output;
                    #safety fn resolve(&self) -> Self::Output { #call }
                }
            ));
        }
    }
    quote!(#(#emitted)*)
}

pub(crate) fn forwarding(
    parameters: &[Ident],
    children: &[crate::child::ChildRegistration],
) -> TokenStream {
    let tuple_parameter = &parameters[parameters.len() - 2];
    let child_parameters = children
        .iter()
        .enumerate()
        .map(|(i, _)| format_ident!("__NestedChild{i}"))
        .collect::<Vec<_>>();
    let mut emitted = Vec::new();
    emitted.push(quote!(
        pub trait __systasis_ChildRegistrationPolicy<__ChildKey, __Rest, __Key, const __LOCAL: bool>
        {
            type Policy;
        }
        impl<
            '__backing,
            __Container: ?Sized,
            __Restrictions,
            __Children,
            __Path,
            __Key,
            const __LOCAL: bool,
        > ::systasis::scoped::RegistrationPolicy<__Path, __Key, __LOCAL>
            for __systasis_Scope<'__backing, __Container, __Restrictions, __Children>
        where
            __Container: ::systasis::scoped::RegistrationPolicy<__Path, __Key, __LOCAL>,
        {
            type Policy = <__Container as ::systasis::scoped::RegistrationPolicy<
                __Path,
                __Key,
                __LOCAL,
            >>::Policy;
        }
        pub trait __systasis_ChildBorrowed<__ChildKey, __Rest, __Key> {
            type Mask;
        }
        impl<'__backing, __Container: ?Sized, __Restrictions, __Children, __Path, __Key>
            ::systasis::scoped::Borrowed<__Path, __Key>
            for __systasis_Scope<'__backing, __Container, __Restrictions, __Children>
        where
            __Container: ::systasis::scoped::Borrowed<__Path, __Key>,
        {
            type Mask = <__Container as ::systasis::scoped::Borrowed<__Path, __Key>>::Mask;
        }
    ));
    for (index, child) in children.iter().enumerate() {
        let child_key = crate::scopegen::key(&child.name.unraw().to_string());
        let child_parameter = &child_parameters[index];
        emitted.push(quote!(
            impl<#(#child_parameters,)* __Rest, __Key, const __LOCAL: bool>
                __systasis_ChildRegistrationPolicy<#child_key, __Rest, __Key, __LOCAL> for (#(#child_parameters,)*)
            where #child_parameter: ::systasis::scoped::RegistrationPolicy<__Rest, __Key, __LOCAL> {
                type Policy = <#child_parameter as ::systasis::scoped::RegistrationPolicy<__Rest, __Key, __LOCAL>>::Policy;
            }
            impl<#(#parameters,)* __Rest, __Key, const __LOCAL: bool>
                ::systasis::scoped::RegistrationPolicy<::systasis::scoped::There<#child_key, __Rest>, __Key, __LOCAL>
                for __systasis_Generated<#(#parameters),*>
            where #tuple_parameter: __systasis_ChildRegistrationPolicy<#child_key, __Rest, __Key, __LOCAL> {
                type Policy = <#tuple_parameter as __systasis_ChildRegistrationPolicy<#child_key, __Rest, __Key, __LOCAL>>::Policy;
            }
            impl<#(#child_parameters,)* __Rest, __Key> __systasis_ChildBorrowed<#child_key, __Rest, __Key>
                for (#(#child_parameters,)*)
            where #child_parameter: ::systasis::scoped::Borrowed<__Rest, __Key> {
                type Mask = <#child_parameter as ::systasis::scoped::Borrowed<__Rest, __Key>>::Mask;
            }
            impl<#(#parameters,)* __Rest, __Key>
                ::systasis::scoped::Borrowed<::systasis::scoped::There<#child_key, __Rest>, __Key>
                for __systasis_Generated<#(#parameters),*>
            where #tuple_parameter: __systasis_ChildBorrowed<#child_key, __Rest, __Key> {
                type Mask = ::systasis::scoped::mask::Nested<#child_key,
                    <#tuple_parameter as __systasis_ChildBorrowed<#child_key, __Rest, __Key>>::Mask>;
            }
        ));
    }
    for (public, helper, associated, extra) in [
        ("Registered", "__systasis_ChildRegistered", "Value", false),
        (
            "DynRegistered",
            "__systasis_ChildDynRegistered",
            "Target",
            false,
        ),
        ("Output", "__systasis_ChildOutput", "Value", true),
    ] {
        let public = format_ident!("{public}");
        let helper = format_ident!("{helper}");
        let associated = format_ident!("{associated}");
        let operation = extra.then(|| quote!(, __Operation));
        let sized = (public == "DynRegistered").then(|| quote!(: ?Sized));
        // A dyn target's object bound follows the requested borrow, not the
        // longer lifetime of a leaf behind an intermediate descriptor. Owned
        // constructor outputs still retain their original backing lifetime.
        let target_lifetime = if public == "DynRegistered" {
            quote!('a)
        } else {
            quote!('__backing)
        };
        emitted.push(quote!(
            pub trait #helper<__ChildKey, __Rest, __Key #operation> {
                type #associated<'a> #sized where Self: 'a;
            }
            impl<'__backing, __Container: ?Sized + '__backing, __Restrictions, __Children, __Path, __Key #operation>
                ::systasis::scoped::#public<__Path, __Key #operation>
                for __systasis_Scope<'__backing, __Container, __Restrictions, __Children>
            where __Container: ::systasis::scoped::#public<__Path, __Key #operation> {
                type #associated<'a> = <__Container as ::systasis::scoped::#public<__Path, __Key #operation>>::#associated<#target_lifetime> where Self: 'a;
            }
        ));
        for (index, child) in children.iter().enumerate() {
            let child_key = crate::scopegen::key(&child.name.unraw().to_string());
            let child_parameter = &child_parameters[index];
            emitted.push(quote!(
                impl<#(#child_parameters,)* __Rest, __Key #operation> #helper<#child_key, __Rest, __Key #operation>
                    for (#(#child_parameters,)*)
                where #child_parameter: ::systasis::scoped::#public<__Rest, __Key #operation> {
                    type #associated<'a> = <#child_parameter as ::systasis::scoped::#public<__Rest, __Key #operation>>::#associated<'a> where Self: 'a;
                }
                impl<#(#parameters,)* __Rest, __Key #operation>
                    ::systasis::scoped::#public<::systasis::scoped::There<#child_key, __Rest>, __Key #operation>
                    for __systasis_Generated<#(#parameters),*>
                where #tuple_parameter: #helper<#child_key, __Rest, __Key #operation> {
                    type #associated<'a> = <#tuple_parameter as #helper<#child_key, __Rest, __Key #operation>>::#associated<'a> where Self: 'a;
                }
            ));
        }
    }
    let mut operations = vec![("Resolve", "__systasis_ChildResolve", false)];
    if cfg!(feature = "resolve_unchecked") {
        operations.push(("UnsafeResolve", "__systasis_ChildUnsafeResolve", true));
    }
    for (public, helper, unchecked) in operations {
        let public = format_ident!("{public}");
        let helper = format_ident!("{helper}");
        let safety = unchecked.then(|| quote!(unsafe));
        emitted.push(quote!(
            pub trait #helper<'__backing, __ChildKey, __Rest, __Key, __Operation> {
                type Output;
                #safety fn resolve(&self) -> Self::Output;
            }
        ));
        for (index, child) in children.iter().enumerate() {
            let child_key = crate::scopegen::key(&child.name.unraw().to_string());
            let child_parameter = &child_parameters[index];
            let position = Index::from(index);
            let child_call = quote!(<#child_parameter as ::systasis::scoped::#public<'__backing, __Rest, __Key, __Operation>>::resolve(&self.#position));
            let scope_call = quote!(<__Children as #helper<'__backing, #child_key, __Rest, __Key, __Operation>>::resolve(&self.children));
            let wrap = |call| {
                if unchecked {
                    quote!({
                        // SAFETY: forward the caller's unchanged acquisition preconditions
                        // through the stored restricted child; no exclusion is removed.
                        unsafe { #call }
                    })
                } else {
                    call
                }
            };
            let child_call = wrap(child_call);
            let scope_call = wrap(scope_call);
            emitted.push(quote!(
                impl<'__backing, #(#child_parameters,)* __Rest, __Key, __Operation>
                    #helper<'__backing, #child_key, __Rest, __Key, __Operation> for (#(#child_parameters,)*)
                where #child_parameter: ::systasis::scoped::#public<'__backing, __Rest, __Key, __Operation> {
                    type Output = <#child_parameter as ::systasis::scoped::#public<'__backing, __Rest, __Key, __Operation>>::Output;
                    #safety fn resolve(&self) -> Self::Output { #child_call }
                }
                impl<'__backing, __Container: ?Sized, __Restrictions, __Children, __Rest, __Key, __Operation>
                    ::systasis::scoped::#public<'__backing, ::systasis::scoped::There<#child_key, __Rest>, __Key, __Operation>
                    for __systasis_Scope<'__backing, __Container, __Restrictions, __Children>
                where __Children: #helper<'__backing, #child_key, __Rest, __Key, __Operation> {
                    type Output = <__Children as #helper<'__backing, #child_key, __Rest, __Key, __Operation>>::Output;
                    #safety fn resolve(&self) -> Self::Output { #scope_call }
                }
            ));
        }
    }
    quote!(#(#emitted)*)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn namespace_forwarding_deduplicates_groups_and_default_spelling() {
        let parameters = vec![
            syn::parse_quote!(__Slot),
            syn::parse_quote!(__Children),
            syn::parse_quote!(__Generic),
        ];
        let crate::parse::Registrations(registrations) = syn::parse_str("register_value!(1: u32 as IA); register_value!(2: u32 as IB in default); register_value!(3: u32 as IA in metrics);").unwrap();
        let emitted = namespaces(&parameters, &registrations);
        let parsed: syn::File = syn::parse2(emitted).unwrap();
        let expected_per_namespace = if cfg!(feature = "resolve_unchecked") {
            7
        } else {
            6
        };
        assert_eq!(parsed.items.len(), 2 * expected_per_namespace);
    }

    #[test]
    fn emitted_forwarding_is_valid_rust_for_empty_and_nested_children() {
        let parameters = vec![
            syn::parse_quote!(__Slot),
            syn::parse_quote!(__Children),
            syn::parse_quote!(__Generic),
        ];
        for children in [
            Vec::new(),
            vec![
                syn::parse_str("primary: &Child").unwrap(),
                syn::parse_str("secondary: &Child").unwrap(),
            ],
        ] {
            syn::parse2::<syn::File>(forwarding(&parameters, &children)).unwrap();
        }
    }
}
