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
        if build.method != "build" || !registry.mac.path.is_ident("systasis_container") {
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
            .map(|i| format_ident!("__systasis_slot_{i}"))
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
            let mut probe = Lifetimes(Vec::new());
            probe.visit_type_mut(&mut original.clone());
            let flag = &flags[i];
            let interface = &registration.interface;
            let default_bound = registration
                .fresh
                .then(|| quote!(+ ::core::default::Default));
            constants.push(if probe.0.is_empty() {
                quote!(pub(super) const #flag: bool = {
                    fn __systasis_check<T: #interface #default_bound>() {}
                    let _ = __systasis_check::<#original>;
                    ::systasis::__private::Pick::<#original>::IS_COPY
                };)
            } else {
                quote!(pub(super) const #flag: bool = false;)
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
            values.push(quote!(#field: #slot.unwrap_or_else(|| unreachable!("successful build initialized every slot"))));
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
            implementations.push(quote!(
                impl<__Value: ::core::default::Default, #(#others),*> Generated<#(#fresh_args),*> {
                    pub fn #copy(&self) -> __Value { self.#field.resolve() }
                }
                impl<#lifetime __Value: Copy, #(#others),*> Generated<#(#copy_args),*> {
                    pub fn #copy(&self) -> __Value { self.#field.resolve() }
                    pub fn #copy_ref(&self) -> &__Value { self.#field.resolve_ref() }
                    pub fn #clone(&self) -> __Value { self.#field.resolve_clone() }
                }
                impl<#lifetime __Value, #(#others),*> Generated<#(#take_args),*> {
                    pub fn #read(&self) -> Result<#read_type<'_,__Value>,::systasis::__private::Error> { self.#field.try_resolve_ref() }
                    pub fn #write(&self) -> Result<#write_type<'_,__Value>,::systasis::__private::Error> { self.#field.try_resolve_ref_mut() }
                    pub fn #take(&self) -> Result<__Value,::systasis::__private::Error> { self.#field.try_resolve() }
                }
            ));
        }
        emitted = Some(quote!(
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
            let ty = &registrations[*i].ty;
            let stored = if registrations[*i].fresh {
                quote!(::systasis::__private::FreshSlot::<#ty>::new())
            } else {
                quote!(<::systasis::__private::Policy<{__systasis_injected::#flag}, #local_policy> as ::systasis::__private::Select<_>>::store({#value}))
            };
            initialization.push(quote!(let (#slot,__systasis_error)=match __systasis_error {
                Some(error)=>(None,Some(error)),
                None=>::systasis::__private::split((|| -> Result<_, #error_ty> {
                    // An empty match coerces into the contextual error type.
                    // It cannot execute: this expression constructs Ok, whose
                    // source error type Infallible has no inhabitants.
                    Result::<_, ::core::convert::Infallible>::Ok(
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
                if let Ok(container) = &__systasis_published { __systasis_assert(*container); }
            })
        });
        let generated: Block = syn::parse2(quote!({
            let __systasis_error:Option<#error_ty>=Result::<(), ::core::convert::Infallible>::Ok(()).map_err(|never| match never {}).err();
            #(#initialization)*
            let __systasis_result=match __systasis_error {
                None=>Ok(AppContainer {#(#values,)*_pin: ::core::marker::PhantomPinned,_parameters: ::core::marker::PhantomData}),
                Some(error)=>{#(::systasis::__private::discard(#reverse);)* Err(error)},
            };
            let (__systasis_value,__systasis_error)=::systasis::__private::split(__systasis_result);
            let __systasis_owner=::core::pin::pin!(__systasis_value);
            let __systasis_published=match (__systasis_owner.as_ref().get_ref(),__systasis_error) {
                (Some(container),None)=>Ok(container),
                (None,Some(error))=>Err(error),
                _=>unreachable!("split result has exactly one occupied branch"),
            };
            #(#checks)*
            let #pattern = __systasis_published #otherwise;
        }))?;
        statements.extend(generated.stmts);
    }
    function.block.stmts = statements;
    Ok(quote!(#emitted #function))
}
