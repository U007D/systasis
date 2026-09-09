use crate::analysis::{Queries, TypeLookup};
use crate::parse::Registrations;
use proc_macro2::Span;
use quote::{ToTokens, format_ident, quote};
use std::collections::{BTreeMap, BTreeSet};
use syn::{visit_mut::VisitMut, *};
struct Lifetimes(Vec<Lifetime>);
impl VisitMut for Lifetimes {
    fn visit_lifetime_mut(&mut self, lifetime: &mut Lifetime) {
        if lifetime.ident == "_" {
            *lifetime = Lifetime::new(
                &format!("'__systasis_type_{}", self.0.len()),
                Span::mixed_site(),
            );
            self.0.push(lifetime.clone());
        }
    }
}

pub(crate) fn expand(
    mut function: ItemFn,
    local_policy: bool,
    requirements: &[Ident],
) -> Result<proc_macro2::TokenStream> {
    if !function.sig.generics.params.is_empty() {
        return Err(Error::new_spanned(
            &function.sig.generics,
            "generic container functions are not implemented yet",
        ));
    }
    let systasis_error_ident = Ident::new("__systasis_error", Span::mixed_site());
    let systasis_value_ident = Ident::new("__systasis_value", Span::mixed_site());
    let systasis_result_ident = Ident::new("__systasis_result", Span::mixed_site());
    let systasis_owner_ident = Ident::new("__systasis_owner", Span::mixed_site());
    let systasis_published_ident = Ident::new("__systasis_published", Span::mixed_site());
    let slot_type = if local_policy {
        quote!(::systasis::__private::LocalTakeSlot)
    } else {
        quote!(::systasis::__private::TakeSlot)
    };
    let marker = if local_policy {
        quote!(::core::cell::Cell<()>)
    } else {
        quote!(())
    };
    let read_type = if local_policy {
        quote!(::core::cell::Ref)
    } else {
        quote!(::systasis::__private::Ref)
    };
    let write_type = if local_policy {
        quote!(::core::cell::RefMut)
    } else {
        quote!(::systasis::__private::RefMut)
    };
    let mut emitted = None;
    let mut statements = Vec::new();
    for statement in &function.block.stmts {
        let Stmt::Local(local) = statement else {
            statements.push(statement.clone());
            continue;
        };
        let Some(initializer) = &local.init else {
            statements.push(statement.clone());
            continue;
        };
        let Expr::MethodCall(build) = &*initializer.expr else {
            statements.push(statement.clone());
            continue;
        };
        let Expr::Macro(registry) = &*build.receiver else {
            statements.push(statement.clone());
            continue;
        };
        if build.method != "build"
            || !registry
                .mac
                .path
                .segments
                .last()
                .is_some_and(|segment| segment.ident == "systasis_container")
        {
            statements.push(statement.clone());
            continue;
        }
        if emitted.is_some() {
            return Err(Error::new_spanned(
                build,
                "only one container definition per function is currently supported",
            ));
        }
        if !build.args.is_empty()
            || build
                .turbofish
                .as_ref()
                .is_some_and(|args| args.args.len() != 1)
        {
            return Err(Error::new_spanned(
                build,
                "build accepts no arguments and at most one error type",
            ));
        }
        let Registrations(mut registrations) = syn::parse2(registry.mac.tokens.clone())?;
        // Discard superseded declarations before examining their expressions,
        // types, or dependencies. The last declaration of an interface wins.
        let winners = registrations
            .iter()
            .enumerate()
            .map(|(i, registration)| (registration.interface.to_token_stream().to_string(), i))
            .collect::<BTreeMap<_, _>>();
        registrations = registrations
            .into_iter()
            .enumerate()
            .filter_map(|(i, registration)| {
                (winners[&registration.interface.to_token_stream().to_string()] == i)
                    .then_some(registration)
            })
            .collect();
        let error_ty = build
            .turbofish
            .as_ref()
            .and_then(|a| a.args.first())
            .cloned()
            .unwrap_or(parse_quote!(_));
        let indices = registrations
            .iter()
            .enumerate()
            .map(|(i, r)| (r.interface.to_token_stream().to_string(), i))
            .collect::<BTreeMap<_, _>>();
        let mut dependencies = Vec::new();
        let original_types = registrations
            .iter()
            .map(|r| r.ty.clone())
            .collect::<Vec<_>>();
        for registration in &mut registrations {
            let mut lookup = TypeLookup {
                indices: &indices,
                types: &original_types,
                active: Vec::new(),
                dependencies: BTreeSet::new(),
                error: None,
            };
            lookup.visit_type_mut(&mut registration.ty);
            lookup.visit_expr_mut(&mut registration.value);
            if let Some(error) = lookup.error {
                return Err(error);
            }
            let mut queries = Queries {
                indices: &indices,
                dependencies: lookup.dependencies,
                error: None,
            };
            queries.visit_expr_mut(&mut registration.value);
            if let Some(error) = queries.error {
                return Err(error);
            }
            dependencies.push(queries.dependencies);
        }
        let initializer_types = registrations
            .iter()
            .map(|r| r.ty.clone())
            .collect::<Vec<_>>();
        let order = crate::graph::schedule(&dependencies).map_err(|cycle| {
            let names = cycle
                .iter()
                .map(|&i| registrations[i].interface.to_token_stream().to_string())
                .collect::<Vec<_>>();
            Error::new_spanned(
                registry,
                format!("registration dependency cycle: {}", names.join(" -> ")),
            )
        })?;
        let slots = (0..registrations.len())
            .map(|i| format_ident!("__systasis_slot_{i}", span = Span::mixed_site()))
            .collect::<Vec<_>>();
        let fields = (0..registrations.len())
            .map(|i| format_ident!("value_{i}"))
            .collect::<Vec<_>>();

        let flags = (0..registrations.len())
            .map(|i| format_ident!("COPY_{i}"))
            .collect::<Vec<_>>();
        let mut constants = Vec::new();
        let mut lifetimes = Lifetimes(Vec::new());
        for (i, registration) in registrations.iter_mut().enumerate() {
            let original = &registration.ty;
            let flag = &flags[i];
            let interface = &registration.interface;
            let default_bound = registration
                .fresh
                .then(|| quote!(+ ::core::default::Default));
            constants.push({
                quote!(pub(super) const #flag: bool = {
                    fn __systasis_check<T: #interface #default_bound>() {}
                    let _ = __systasis_check::<#original>;
                    ::systasis::__private::Pick::<#original>::IS_COPY
                };)
            });
            lifetimes.visit_type_mut(&mut registration.ty);
        }
        let selected = registrations.iter().enumerate().map(|(i,r)| {
            let ty = &r.ty;
            let flag = &flags[i];
            if r.fresh { quote!(::systasis::__private::FreshSlot<#ty>) }
            else { quote!(<::systasis::__private::Policy<{__systasis_injected::#flag}, #local_policy> as ::systasis::__private::Select<#ty>>::Slot) }
        }).collect::<Vec<_>>();
        let mut generics = function.sig.generics.clone();
        for lifetime in &lifetimes.0 {
            generics.params.insert(0, parse_quote!(#lifetime));
        }
        let parameters = (0..registrations.len())
            .map(|i| format_ident!("__Slot{i}"))
            .collect::<Vec<_>>();
        let mut field_types = Vec::new();
        let mut values = Vec::new();
        let mut implementations = Vec::new();
        let mut method_names = BTreeMap::<String, Path>::new();
        for (i, registration) in registrations.iter().enumerate() {
            let field = &fields[i];
            let slot = &slots[i];
            let storage = &selected[i];
            field_types.push(storage.clone());
            values.push(quote!(#field: #slot.unwrap_or_else(|| ::core::unreachable!("successful build initialized every slot"))));
            let raw = registration
                .interface
                .segments
                .last()
                .unwrap()
                .ident
                .to_string();
            let snake = raw
                .chars()
                .enumerate()
                .flat_map(|(i, c)| {
                    if c.is_uppercase() && i > 0 {
                        vec!['_', c.to_ascii_lowercase()]
                    } else {
                        vec![c.to_ascii_lowercase()]
                    }
                })
                .collect::<String>();
            if let Some(previous) =
                method_names.insert(snake.clone(), registration.interface.clone())
            {
                let mut error = Error::new_spanned(
                    &registration.interface,
                    "interfaces generate the same resolver name",
                );
                error.combine(Error::new_spanned(previous, "first conflicting interface"));
                return Err(error);
            }
            let read = format_ident!("try_resolve_{snake}_ref");
            let write = format_ident!("try_resolve_{snake}_ref_mut");
            let take = format_ident!("try_resolve_{snake}");
            let copy = format_ident!("resolve_{snake}");
            let copy_ref = format_ident!("resolve_{snake}_ref");
            let clone = format_ident!("resolve_{snake}_clone");
            let try_clone = format_ident!("try_resolve_{snake}_clone");
            let others = parameters
                .iter()
                .enumerate()
                .filter(|(j, _)| *j != i)
                .map(|(_, p)| p)
                .collect::<Vec<_>>();
            let lifetime = quote!();
            let reference = quote!();
            let copy_args = parameters
                .iter()
                .enumerate()
                .map(|(j, p)| {
                    if i == j {
                        quote!(#reference ::systasis::__private::CopySlot<__Value>)
                    } else {
                        quote!(#p)
                    }
                })
                .collect::<Vec<_>>();
            let take_args = parameters
                .iter()
                .enumerate()
                .map(|(j, p)| {
                    if i == j {
                        quote!(#reference #slot_type<__Value>)
                    } else {
                        quote!(#p)
                    }
                })
                .collect::<Vec<_>>();
            let fresh_args = parameters
                .iter()
                .enumerate()
                .map(|(j, p)| {
                    if i == j {
                        quote!(::systasis::__private::FreshSlot<__Value>)
                    } else {
                        quote!(#p)
                    }
                })
                .collect::<Vec<_>>();
            if registration.dynamic {
                let interface = &registration.interface;
                let dyn_ref = format_ident!("try_resolve_{snake}_dyn_ref");
                let copy_dyn_ref = format_ident!("resolve_{snake}_dyn_ref");
                implementations.push(quote!(
                    impl<__Value: #interface, #(#others),*> Generated<#(#take_args),*> {
                        pub fn #dyn_ref(&self) -> ::core::result::Result<#read_type<'_, dyn #interface + '_>, ::systasis::__private::Error> {
                            self.#field.try_resolve_ref().map(|guard| #read_type::map(guard, |value| value as &(dyn #interface + '_)))
                        }
                    }
                    impl<__Value: #interface + ::core::marker::Copy, #(#others),*> Generated<#(#copy_args),*> {
                        pub fn #copy_dyn_ref(&self) -> &(dyn #interface + '_) { self.#field.resolve_ref() }
                    }
                ));
            }
            implementations.push(quote!(
                impl<__Value: ::core::default::Default, #(#others),*> Generated<#(#fresh_args),*> {
                    pub fn #copy(&self) -> __Value { self.#field.resolve() }
                }
                impl<#lifetime __Value: ::core::marker::Copy, #(#others),*> Generated<#(#copy_args),*> {
                    pub fn #copy(&self) -> __Value { self.#field.resolve() }
                    pub fn #copy_ref(&self) -> &__Value { self.#field.resolve_ref() }
                    pub fn #clone(&self) -> __Value { self.#field.resolve_clone() }
                }
                impl<#lifetime __Value, #(#others),*> Generated<#(#take_args),*> {
                    pub fn #read(&self) -> ::core::result::Result<#read_type<'_,__Value>,::systasis::__private::Error> { self.#field.try_resolve_ref() }
                    pub fn #write(&self) -> ::core::result::Result<#write_type<'_,__Value>,::systasis::__private::Error> { self.#field.try_resolve_ref_mut() }
                    pub fn #take(&self) -> ::core::result::Result<__Value,::systasis::__private::Error> { self.#field.try_resolve() }
                    pub fn #try_clone(&self) -> ::core::result::Result<__Value,::systasis::__private::Error>
                    where __Value: ::core::clone::Clone { self.#field.try_resolve_clone() }
                }
            ));
        }
        emitted = ::core::option::Option::Some(quote!(
            #[allow(non_snake_case, unused_imports, dead_code)]
            mod __systasis_injected {
                use super::*;
                use ::systasis::__private::CopyFallback as _;
                #(#constants)*
                pub struct Generated<#(#parameters),*> {
                    #(pub(super) #fields: #parameters,)*
                    pub(super) _pin: ::core::marker::PhantomPinned,
                    pub(super) _parameters: ::core::marker::PhantomData<#marker>,
                }
                #(#implementations)*
            }
            /// The container generated from this module's registration declaration.
            pub type AppContainer #generics = __systasis_injected::Generated<#(#field_types),*>;
        ));
        let mut initialization = Vec::new();
        for i in &order {
            let slot = &slots[*i];
            let value = &registrations[*i].value;
            let flag = &flags[*i];
            let ty = &initializer_types[*i];
            let interface = &registrations[*i].interface;
            let stored = if registrations[*i].fresh {
                quote!(::systasis::__private::FreshSlot::<#ty>::new())
            } else {
                quote!({
                    let __systasis_input: #ty = { #value };
                    {
                        fn __systasis_check<T: #interface>(value: T) -> T { value }
                        <::systasis::__private::Policy<{__systasis_injected::#flag}, #local_policy> as ::systasis::__private::Select<#ty>>::store(__systasis_check(__systasis_input))
                    }
                })
            };
            initialization.push(quote!(let (#slot,#systasis_error_ident)=match #systasis_error_ident {
                ::core::option::Option::Some(error)=>(::core::option::Option::None,::core::option::Option::Some(error)),
                ::core::option::Option::None=>::systasis::__private::split((|| -> ::core::result::Result<_, #error_ty> {
                    // An empty match coerces into the contextual error type.
                    // It cannot execute: this expression constructs Ok, whose
                    // source error type Infallible has no inhabitants.
                    ::core::result::Result::<_, ::core::convert::Infallible>::Ok(
                        #stored
                    ).map_err(|never| match never {})
                })()),
            };));
        }
        let reverse = order.iter().rev().map(|i| &slots[*i]);
        let pattern = &local.pat;
        let otherwise = initializer
            .diverge
            .as_ref()
            .map(|(_, expression)| quote!(else #expression));
        let checks = requirements.iter().map(|bound| {
            quote!({
                fn __systasis_assert<T: ::core::marker::#bound>(_: &T) {}
                if let ::core::result::Result::Ok(container) = &#systasis_published_ident { __systasis_assert(*container); }
            })
        });
        let macro_path = &registry.mac.path;
        let generated: Block = syn::parse2(quote!({
            #macro_path!(@__systasis_marker);
            let #systasis_error_ident: ::core::option::Option<#error_ty>=::core::result::Result::<(), ::core::convert::Infallible>::Ok(()).map_err(|never| match never {}).err();
            #(#initialization)*
            let #systasis_result_ident=match #systasis_error_ident {
                ::core::option::Option::None=>::core::result::Result::Ok(AppContainer {#(#values,)*_pin: ::core::marker::PhantomPinned,_parameters: ::core::marker::PhantomData}),
                ::core::option::Option::Some(error)=>{#(::systasis::__private::discard(#reverse);)* ::core::result::Result::Err(error)},
            };
            let (#systasis_value_ident,#systasis_error_ident)=::systasis::__private::split(#systasis_result_ident);
            let #systasis_owner_ident=::core::pin::pin!(#systasis_value_ident);
            let #systasis_published_ident=match (#systasis_owner_ident.as_ref().get_ref(),#systasis_error_ident) {
                (::core::option::Option::Some(container),::core::option::Option::None)=>::core::result::Result::Ok(container),
                (::core::option::Option::None,::core::option::Option::Some(error))=>::core::result::Result::Err(error),
                _=>::core::unreachable!("split result has exactly one occupied branch"),
            };
            #(#checks)*
            let #pattern = #systasis_published_ident #otherwise;
        }))?;
        statements.extend(generated.stmts);
    }
    function.block.stmts = statements;
    ::core::result::Result::Ok(quote!(#emitted #function))
}
