//! Generate child-free restricted descriptors from the existing resolver impls.

use std::collections::{BTreeMap, BTreeSet};

use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote};
use syn::{
    ext::IdentExt,
    parse::Parser,
    visit_mut::{self, VisitMut},
    *,
};

use crate::parse::{InterfaceGroup, Namespace, Registration};

/// Exact UTF-8 key, balanced to avoid a type-recursion depth linear in length.
pub(crate) fn key(text: &str) -> Type {
    let mut leaves = text
        .bytes()
        .flat_map(|byte| {
            (0..8).map(move |bit| {
                if byte & (1 << bit) == 0 {
                    quote!(::systasis::scoped::key::Zero)
                } else {
                    quote!(::systasis::scoped::key::One)
                }
            })
        })
        .collect::<Vec<_>>();
    leaves.resize(
        leaves.len().max(1).next_power_of_two(),
        quote!(::systasis::scoped::key::Pad),
    );
    while leaves.len() > 1 {
        leaves = leaves
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| {
                let left = &pair[0];
                let right = &pair[1];
                quote!(::systasis::scoped::key::Pair<#left, #right>)
            })
            .collect();
    }
    syn::parse2(
        leaves
            .pop()
            .unwrap_or_else(|| unreachable!("key encoding always contains at least one leaf")),
    )
    .unwrap_or_else(|_| {
        unreachable!("key encoding emits only fixed type paths and balanced generic arguments")
    })
}

pub(crate) fn keys(registration: &Registration) -> (Type, Type, Type) {
    let group = key(&registration.interface.key());
    let namespace = key(&registration
        .namespace
        .0
        .as_ref()
        .map(|name| name.unraw().to_string())
        .unwrap_or_else(|| "default".into()));
    let path = if registration.namespace.0.is_none() {
        parse_quote!(::systasis::scoped::Here)
    } else {
        parse_quote!(::systasis::scoped::Here<#namespace>)
    };
    let restriction = parse_quote!(::systasis::scoped::key::Pair<#namespace, #group>);
    (group, path, restriction)
}

/// Constrain a generic result through type equality rather than putting a
/// private output type in a public associated-type definition or container alias.
fn metadata_at(
    generics: &Generics,
    root: &Type,
    entry: &Entry<'_>,
    kind: TokenStream,
    lifetime: &Lifetime,
    output: &Type,
    allow_unsized: bool,
) -> TokenStream {
    let result = crate::dyn_targets::value_parameter(generics);
    let mut generics = generics.clone();
    generics.params.insert(0, parse_quote!(#lifetime));
    generics.params.push(if allow_unsized {
        parse_quote!(#result: ?Sized)
    } else {
        parse_quote!(#result)
    });
    generics
        .make_where_clause()
        .predicates
        .push(parse_quote!(#root: #lifetime));
    generics
        .make_where_clause()
        .predicates
        .push(parse_quote!(#output: ::systasis::scoped::Identity<Type = #result>));
    let (parameters, _, constraints) = generics.split_for_impl();
    let path = entry.path;
    let key = entry.key;
    quote!(
        impl #parameters ::systasis::scoped::MetadataAt<#lifetime, #path, #key, #kind> for #root #constraints {
            type Value = #result;
        }
    )
}

pub(crate) fn descriptor(
    parameters: &[Ident],
    children: &[crate::child::ChildRegistration],
) -> TokenStream {
    let child_parameter = &parameters[parameters.len() - 2];
    let child_parameters = children
        .iter()
        .enumerate()
        .map(|(i, _)| format_ident!("__Child{i}"))
        .collect::<Vec<_>>();
    let keys = children
        .iter()
        .map(|child| key(&child.name.unraw().to_string()))
        .collect::<Vec<_>>();
    let mask_bounds = keys
        .iter()
        .map(|key| quote!(__Restrictions: ::systasis::scoped::mask::ForChild<#key>));
    let child_bounds = child_parameters.iter().zip(&keys).map(|(child,key)| quote!(#child: ::systasis::scoped::ReScope<<__Restrictions as ::systasis::scoped::mask::ForChild<#key>>::Out>));
    let outputs = child_parameters.iter().zip(&keys).map(|(child,key)| quote!(<#child as ::systasis::scoped::ReScope<<__Restrictions as ::systasis::scoped::mask::ForChild<#key>>::Out>>::Scope));
    let values = keys.iter().enumerate().map(|(i,key)| { let position = Index::from(i); quote!(::systasis::scoped::ReScope::<<__Restrictions as ::systasis::scoped::mask::ForChild<#key>>::Out>::rescope(&self.#position)) });
    let restricted_body = if children.is_empty() {
        quote!()
    } else {
        quote!((#(#values,)*))
    };
    let accessors = children
        .iter()
        .zip(&child_parameters)
        .enumerate()
        .map(|(i, (child, ty))| {
            let name = &child.name;
            let position = Index::from(i);
            quote!(pub fn #name(&self) -> &#ty { &self.children.#position })
        });
    quote!(
        pub trait __ScopeChildren<__Restrictions> {
            type Output;
            fn restricted(&self) -> Self::Output;
        }
        impl<__Restrictions, #(#child_parameters),*> __ScopeChildren<__Restrictions> for (#(#child_parameters,)*)
        where #(#mask_bounds,)* #(#child_bounds,)* {
            type Output = (#(#outputs,)*);
            fn restricted(&self) -> Self::Output { #restricted_body }
        }
        pub struct __SystasisScope<'__backing, __Container: ?Sized, __Restrictions, __Children = ()> {
            backing: &'__backing __Container,
            restrictions: ::core::marker::PhantomData<fn() -> __Restrictions>,
            children: __Children,
        }
        impl<#(#parameters,)* __Restrictions> ::systasis::scoped::AsScope<__Restrictions> for Generated<#(#parameters),*>
        where #child_parameter: __ScopeChildren<__Restrictions> {
            type Scope<'__backing> = __SystasisScope<'__backing, Self, __Restrictions, <#child_parameter as __ScopeChildren<__Restrictions>>::Output> where Self: '__backing;
            fn scope(&self) -> Self::Scope<'_> {
                __SystasisScope { backing: self, restrictions: ::core::marker::PhantomData, children: self._children.restricted() }
            }
        }
        impl<'__backing, #(#parameters,)* __Restrictions, __More, __Children> ::systasis::scoped::ReScope<__More> for __SystasisScope<'__backing, Generated<#(#parameters),*>, __Restrictions, __Children>
        where #child_parameter: __ScopeChildren<::systasis::scoped::mask::Union<__Restrictions, __More>> {
            type Scope = <Generated<#(#parameters),*> as ::systasis::scoped::AsScope<::systasis::scoped::mask::Union<__Restrictions, __More>>>::Scope<'__backing>;
            fn rescope(&self) -> Self::Scope {
                ::systasis::scoped::AsScope::<::systasis::scoped::mask::Union<__Restrictions, __More>>::scope(self.backing)
            }
        }
        impl<'__backing, __Container: ?Sized, __Restrictions, #(#child_parameters),*> __SystasisScope<'__backing, __Container, __Restrictions, (#(#child_parameters,)*)> {
            #(#accessors)*
        }
    )
}

pub(crate) fn value_metadata(
    entry: &Entry<'_>,
    parameters: &[Ident],
    index: usize,
    generics: &Generics,
    selected: &[TokenStream],
    declared: &Type,
    dynamic: Option<&Type>,
) -> TokenStream {
    let key = entry.key;
    let path = entry.path;
    let restriction = entry.own_mask_key;
    let (generic_parameters, _, constraints) = generics.split_for_impl();
    let lifetime = fresh_lifetime("__systasis_value", generics);
    let mut declared = declared.clone();
    OutputLifetime {
        replacement: lifetime.clone(),
        method_lifetimes: BTreeSet::new(),
    }
    .visit_type_mut(&mut declared);
    let borrow_metadata = quote!(
        impl<#(#parameters),*> ::systasis::scoped::Borrowed<#path, #key> for Generated<#(#parameters),*> {
            type Mask = ::systasis::scoped::mask::Mask<#restriction, ::systasis::scoped::mask::Empty>;
        }
    );
    let policy_metadata = if entry.registration.fresh {
        let local = crate::dyn_targets::value_parameter(generics);
        let mut policy_generics = generics.clone();
        policy_generics
            .params
            .push(parse_quote!(const #local: bool));
        let policy = entry.policy;
        policy_generics
            .make_where_clause()
            .predicates
            .push(parse_quote!(#policy: ::systasis::scoped::RebindPolicy<#local>));
        let (parameters, _, constraints) = policy_generics.split_for_impl();
        quote!(
            impl #parameters ::systasis::scoped::RegistrationPolicy<#path, #key, #local> for Generated<#(#selected),*> #constraints {
                type Policy = <#policy as ::systasis::scoped::RebindPolicy<#local>>::Policy;
            }
        )
    } else {
        let slot = &parameters[index];
        quote!(
            impl<#(#parameters,)* const __SystasisLocal: bool> ::systasis::scoped::RegistrationPolicy<#path, #key, __SystasisLocal> for Generated<#(#parameters),*>
            where #slot: ::systasis::scoped::SlotPolicy<__SystasisLocal> {
                type Policy = <#slot as ::systasis::scoped::SlotPolicy<__SystasisLocal>>::Policy;
            }
        )
    };
    let registration = if entry.registration.constructor.is_some() {
        let root = parse_quote!(Generated<#(#selected),*>);
        let metadata = metadata_at(
            generics,
            &root,
            entry,
            quote!(::systasis::scoped::RegisteredValue),
            &lifetime,
            &declared,
            false,
        );
        quote!(
            #metadata
            impl #generic_parameters ::systasis::scoped::Registered<#path, #key> for Generated<#(#selected),*> #constraints {
                type Value<#lifetime> = <Self as ::systasis::scoped::MetadataAt<#lifetime, #path, #key, ::systasis::scoped::RegisteredValue>>::Value where Self: #lifetime;
            }
        )
    } else {
        let slot = &parameters[index];
        quote!(
            impl<#(#parameters),*> ::systasis::scoped::Registered<#path, #key> for Generated<#(#parameters),*>
            where #slot: ::systasis::scoped::SlotValue {
                type Value<#lifetime> = <#slot as ::systasis::scoped::SlotValue>::Value where Self: #lifetime;
            }
        )
    };
    let dynamic = dynamic.map(|target| {
        let mut target = target.clone();
        OutputLifetime { replacement: lifetime.clone(), method_lifetimes: BTreeSet::new() }.visit_type_mut(&mut target);
        let root = parse_quote!(Generated<#(#selected),*>);
        let metadata = metadata_at(generics, &root, entry, quote!(::systasis::scoped::DynamicTarget), &lifetime, &target, true);
        quote!(
            #metadata
            impl #generic_parameters ::systasis::scoped::DynRegistered<#path, #key> for Generated<#(#selected),*> #constraints {
                type Target<#lifetime> = <Self as ::systasis::scoped::MetadataAt<#lifetime, #path, #key, ::systasis::scoped::DynamicTarget>>::Value where Self: #lifetime;
            }
        )
    });
    quote!(#borrow_metadata #policy_metadata #registration #dynamic)
}

/// Actual consuming queries, propagated only through called constructors.
pub(crate) fn consumed_dependencies(
    registrations: &[Registration],
    indices: &BTreeMap<String, usize>,
    order: &[usize],
) -> Result<Vec<BTreeSet<usize>>> {
    struct Calls<'a> {
        indices: &'a BTreeMap<String, usize>,
        calls: Vec<(usize, bool)>,
        error: Option<Error>,
    }
    impl VisitMut for Calls<'_> {
        fn visit_expr_macro_mut(&mut self, expression: &mut ExprMacro) {
            let Some(name) = expression.mac.path.get_ident().map(ToString::to_string) else {
                return;
            };
            let operation = name.strip_suffix("_from").unwrap_or(&name);
            if !matches!(operation, "resolve" | "try_resolve" | "resolve_unchecked") {
                return;
            }
            let parsed = (|input: syn::parse::ParseStream<'_>| {
                let interface = input.parse::<InterfaceGroup>()?;
                let namespace = Namespace::query(input, name.ends_with("_from"))?;
                Ok(namespace.key(&interface))
            })
            .parse2(expression.mac.tokens.clone());
            match parsed {
                Ok(key) => {
                    if let Some(&index) = self.indices.get(&key) {
                        self.calls.push((index, operation != "resolve"));
                    }
                }
                Err(error) => self.error = Some(error),
            }
        }
    }
    let mut consumed = vec![BTreeSet::new(); registrations.len()];
    for &index in order {
        let Some(constructor) = &registrations[index].constructor else {
            continue;
        };
        let mut calls = Calls {
            indices,
            calls: Vec::new(),
            error: None,
        };
        calls.visit_expr_closure_mut(&mut constructor.clone());
        if let Some(error) = calls.error {
            return Err(error);
        }
        for (target, takes) in calls.calls {
            if registrations[target].constructor.is_some() {
                let inherited = consumed[target].clone();
                consumed[index].extend(inherited);
            } else if takes && !registrations[target].fresh {
                consumed[index].insert(target);
            }
        }
    }
    Ok(consumed)
}

/// Inherit child operations only through executed local constructor queries.
pub(crate) fn propagate_child_calls(
    registrations: &[Registration],
    indices: &BTreeMap<String, usize>,
    order: &[usize],
    calls: &mut [Vec<crate::child_queries::Call>],
) -> Result<()> {
    struct Targets<'a> {
        indices: &'a BTreeMap<String, usize>,
        targets: BTreeSet<usize>,
        error: Option<Error>,
    }
    impl VisitMut for Targets<'_> {
        fn visit_expr_macro_mut(&mut self, expression: &mut ExprMacro) {
            let Some(name) = expression.mac.path.get_ident().map(ToString::to_string) else {
                return;
            };
            if !matches!(
                name.strip_suffix("_from").unwrap_or(&name),
                "resolve" | "try_resolve" | "resolve_unchecked"
            ) {
                return;
            }
            let parsed = (|input: syn::parse::ParseStream<'_>| {
                let interface = input.parse::<InterfaceGroup>()?;
                let namespace = Namespace::query(input, name.ends_with("_from"))?;
                Ok(namespace.key(&interface))
            })
            .parse2(expression.mac.tokens.clone());
            match parsed {
                Ok(key) => {
                    if let Some(&index) = self.indices.get(&key) {
                        self.targets.insert(index);
                    }
                }
                Err(error) => self.error = Some(error),
            }
        }
    }
    for &index in order {
        let Some(constructor) = &registrations[index].constructor else {
            continue;
        };
        let mut targets = Targets {
            indices,
            targets: BTreeSet::new(),
            error: None,
        };
        targets.visit_expr_closure_mut(&mut constructor.clone());
        if let Some(error) = targets.error {
            return Err(error);
        }
        for target in targets.targets {
            if registrations[target].constructor.is_some() {
                let inherited = calls[target].clone();
                calls[index].extend(inherited);
            }
        }
    }
    Ok(())
}

/// Replace method-local/elided output lifetimes with the backing borrow.
struct OutputLifetime {
    replacement: Lifetime,
    method_lifetimes: BTreeSet<String>,
}
impl VisitMut for OutputLifetime {
    fn visit_lifetime_mut(&mut self, lifetime: &mut Lifetime) {
        if lifetime.ident == "_" || self.method_lifetimes.contains(&lifetime.ident.to_string()) {
            *lifetime = self.replacement.clone();
        }
    }
    fn visit_type_reference_mut(&mut self, reference: &mut TypeReference) {
        reference
            .lifetime
            .get_or_insert_with(|| self.replacement.clone());
        visit_mut::visit_type_reference_mut(self, reference);
    }
    fn visit_type_fn_ptr_mut(&mut self, _: &mut TypeFnPtr) {}
    fn visit_parenthesized_generic_arguments_mut(&mut self, _: &mut ParenthesizedGenericArguments) {
    }
    fn visit_trait_bound_mut(&mut self, bound: &mut TraitBound) {
        let saved = self.method_lifetimes.clone();
        if let Some(lifetimes) = &bound.lifetimes {
            for parameter in lifetimes
                .lifetimes
                .iter()
                .filter_map(|parameter| match parameter {
                    GenericParam::Lifetime(parameter) => Some(parameter),
                    _ => None,
                })
            {
                self.method_lifetimes
                    .remove(&parameter.lifetime.ident.to_string());
            }
        }
        visit_mut::visit_path_mut(self, &mut bound.path);
        self.method_lifetimes = saved;
    }
}

fn fresh_lifetime(prefix: &str, generics: &Generics) -> Lifetime {
    (0..)
        .map(|suffix| Lifetime::new(&format!("'{prefix}_{suffix}"), Span::mixed_site()))
        .find(|candidate| {
            !generics
                .lifetimes()
                .any(|parameter| parameter.lifetime.ident == candidate.ident)
        })
        .unwrap_or_else(|| {
            unreachable!("finite declared lifetimes cannot exhaust generated suffixes")
        })
}

fn output_type(method: &ImplItemFn, lifetime: &Lifetime) -> Type {
    let mut output = match &method.sig.output {
        ReturnType::Default => parse_quote!(()),
        ReturnType::Type(_, ty) => *ty.clone(),
    };
    OutputLifetime {
        replacement: lifetime.clone(),
        method_lifetimes: method
            .sig
            .generics
            .lifetimes()
            .map(|parameter| parameter.lifetime.ident.to_string())
            .collect(),
    }
    .visit_type_mut(&mut output);
    output
}

fn operation(
    method: &Ident,
    registration: &Registration,
    snake: &str,
) -> Option<(Ident, bool, bool)> {
    let suffix = registration
        .namespace
        .0
        .as_ref()
        .map(|namespace| format!("_in_{}", namespace.unraw()))
        .unwrap_or_default();
    [
        (format!("resolve_{snake}"), "Owned", false),
        (format!("try_resolve_{snake}"), "TryOwned", true),
        (format!("resolve_{snake}_ref"), "Shared", false),
        (format!("try_resolve_{snake}_ref"), "TryShared", false),
        (
            format!("try_resolve_{snake}_ref_mut"),
            "TryExclusive",
            false,
        ),
        (format!("resolve_{snake}_clone"), "CloneValue", false),
        (format!("try_resolve_{snake}_clone"), "TryCloneValue", false),
        (format!("resolve_{snake}_dyn_ref"), "DynShared", false),
        (
            format!("try_resolve_{snake}_dyn_ref"),
            "TryDynShared",
            false,
        ),
        (format!("resolve_{snake}_unchecked"), "UncheckedOwned", true),
        (
            format!("resolve_{snake}_ref_unchecked"),
            "UncheckedShared",
            false,
        ),
        (
            format!("resolve_{snake}_ref_mut_unchecked"),
            "UncheckedExclusive",
            false,
        ),
    ]
    .into_iter()
    .find_map(|(base, operation, takes)| {
        let primary = format!("{base}{suffix}");
        if *method == primary {
            Some((format_ident!("{operation}"), false, takes))
        } else if registration.namespace.0.is_none() && *method == format!("{primary}_in_default") {
            Some((format_ident!("{operation}"), true, takes))
        } else {
            None
        }
    })
}

pub(crate) struct Entry<'a> {
    pub registration: &'a Registration,
    pub snake: &'a str,
    pub key: &'a Type,
    pub path: &'a Type,
    pub own_mask_key: &'a Type,
    pub consumed_keys: &'a [Type],
    pub child_calls: &'a [crate::child_queries::Call],
    pub policy: &'a TokenStream,
}

/// Emit operation metadata, restricted inherent methods, and typed dispatch.
pub(crate) fn resolvers(implementations: &[TokenStream], entry: &Entry<'_>) -> Result<TokenStream> {
    let mut emitted = Vec::new();
    let key = entry.key;
    let path = entry.path;
    for implementation in implementations {
        let file = syn::parse2::<File>(implementation.clone())?;
        for item in file.items {
            let Item::Impl(implementation) = item else {
                continue;
            };
            let root = &implementation.self_ty;
            for item in &implementation.items {
                let ImplItem::Fn(method) = item else {
                    continue;
                };
                let Some((operation, alias, takes)) =
                    operation(&method.sig.ident, entry.registration, entry.snake)
                else {
                    continue;
                };
                let mut merged = implementation.generics.clone();
                if let Some(clause) = &method.sig.generics.where_clause {
                    merged
                        .make_where_clause()
                        .predicates
                        .extend(clause.predicates.clone());
                }
                let (root_parameters, _, root_where) = merged.split_for_impl();
                let backing = fresh_lifetime("__systasis_backing", &merged);
                let output_lifetime = fresh_lifetime("__systasis_output", &merged);
                let output = output_type(method, &backing);
                let metadata_output = output_type(method, &output_lifetime);
                if !alias {
                    let metadata = metadata_at(
                        &merged,
                        root,
                        entry,
                        quote!(::systasis::scoped::op::#operation),
                        &output_lifetime,
                        &metadata_output,
                        false,
                    );
                    emitted.push(quote!(
                        #metadata
                        impl #root_parameters ::systasis::scoped::Output<#path, #key, ::systasis::scoped::op::#operation> for #root #root_where {
                            type Value<#output_lifetime> = <Self as ::systasis::scoped::MetadataAt<#output_lifetime, #path, #key, ::systasis::scoped::op::#operation>>::Value where Self: #output_lifetime;
                        }
                    ));
                }
                let mut scoped = merged.clone();
                scoped.params.insert(0, parse_quote!(#backing));
                let restrictions = (0..)
                    .map(|suffix| format_ident!("__SystasisRestrictions{suffix}"))
                    .find(|candidate| {
                        !scoped.params.iter().any(|parameter| match parameter {
                            GenericParam::Type(parameter) => parameter.ident == *candidate,
                            GenericParam::Const(parameter) => parameter.ident == *candidate,
                            GenericParam::Lifetime(_) => false,
                        })
                    })
                    .unwrap_or_else(|| {
                        unreachable!("finite generic parameters cannot exhaust identifier suffixes")
                    });
                scoped.params.push(parse_quote!(#restrictions));
                let scope_children = crate::dyn_targets::value_parameter(&scoped);
                scoped.params.push(parse_quote!(#scope_children));
                scoped
                    .make_where_clause()
                    .predicates
                    .push(parse_quote!(#root: #backing));
                let mut blocked = entry.consumed_keys.to_vec();
                if takes && !entry.registration.fresh {
                    blocked.push(entry.own_mask_key.clone());
                }
                for key in blocked {
                    scoped.make_where_clause().predicates.push(parse_quote!(#restrictions: ::systasis::scoped::mask::Blocked<#key, Out = ::systasis::scoped::key::No>));
                }
                for call in entry.child_calls {
                    let child = &call.child;
                    let path = &call.path;
                    let key = &call.key;
                    let operation = &call.operation;
                    let dispatch = if call.unchecked {
                        format_ident!("__ChildUnsafeResolve")
                    } else {
                        format_ident!("__ChildResolve")
                    };
                    scoped.make_where_clause().predicates.push(parse_quote!(#scope_children: #dispatch<#backing, #child, #path, #key, ::systasis::scoped::op::#operation>));
                }
                let (scope_parameters, _, scope_where) = scoped.split_for_impl();
                let name = &method.sig.ident;
                let safety = match method.sig.safety {
                    Safety::Unsafe(token) => Some(token),
                    _ => None,
                };
                let docs = &method.attrs;
                let call = if safety.is_some() {
                    quote!({
                        // SAFETY: this forwarding method preserves the original
                        // operation's caller preconditions and scope exclusions.
                        unsafe { self.backing.#name() }
                    })
                } else {
                    quote!(self.backing.#name())
                };
                emitted.push(quote!(
                    impl #scope_parameters __SystasisScope<#backing, #root, #restrictions, #scope_children> #scope_where {
                        #(#docs)*
                        pub #safety fn #name(&self) -> #output { #call }
                    }
                ));
                if !alias {
                    let result = crate::dyn_targets::value_parameter(&scoped);
                    let mut dispatch_generics = scoped.clone();
                    dispatch_generics.params.push(parse_quote!(#result));
                    dispatch_generics
                        .make_where_clause()
                        .predicates
                        .push(parse_quote!(#output: ::systasis::scoped::Identity<Type = #result>));
                    let (dispatch_parameters, _, dispatch_where) =
                        dispatch_generics.split_for_impl();
                    let dispatch = if safety.is_some() {
                        format_ident!("UnsafeResolve")
                    } else {
                        format_ident!("Resolve")
                    };
                    emitted.push(quote!(
                        impl #dispatch_parameters ::systasis::scoped::#dispatch<#backing, #path, #key, ::systasis::scoped::op::#operation> for __SystasisScope<#backing, #root, #restrictions, #scope_children> #dispatch_where {
                            type Output = #result;
                            #safety fn resolve(&self) -> Self::Output { <#output as ::systasis::scoped::Identity>::into_identity(#call) }
                        }
                    ));
                }
            }
        }
    }
    Ok(quote!(#(#emitted)*))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::Registrations;
    use quote::ToTokens;

    #[test]
    fn only_consuming_value_queries_propagate_through_factory_calls() {
        let Registrations(registrations) = syn::parse_str(
            "register_value!(value: Value as IValue in data);
             register_type_with!(Value as IFirst, try || -> Result<Value, Error> { try_resolve_from!(IValue, data) });
             register_type_with!(Value as ISecond, try || -> Result<Value, Error> { try_resolve!(IFirst) });
             register_type_with!(Value as IShared, || resolve_ref_from!(IValue, data));
             register_type_with!(Value as ICopy, || resolve_from!(IValue, data));
             register_type_with!(Value as IClone, || try_resolve_clone_from!(IValue, data));
             register_type!(Value as IFresh);
             register_type_with!(Value as IUsesFresh, || resolve!(IFresh));",
        ).unwrap();
        let indices = registrations
            .iter()
            .enumerate()
            .map(|(index, registration)| {
                (registration.namespace.key(&registration.interface), index)
            })
            .collect();
        let order = (0..registrations.len()).collect::<Vec<_>>();
        let consumed = consumed_dependencies(&registrations, &indices, &order).unwrap();
        assert_eq!(consumed[1], BTreeSet::from([0]));
        assert_eq!(consumed[2], BTreeSet::from([0]));
        assert!(consumed[3..].iter().all(BTreeSet::is_empty));
    }

    #[test]
    fn output_lifetimes_preserve_explicit_inputs_and_function_pointer_binders() {
        let method: ImplItemFn = parse_quote!(
            fn example<'call>(&'call self) -> (Ref<'call, T>, &'input T, fn(&str) -> &str) {
                unreachable!()
            }
        );
        let lifetime = Lifetime::new("'backing", Span::call_site());
        let expected: Type = parse_quote!((Ref<'backing, T>, &'input T, fn(&str) -> &str));
        assert_eq!(
            output_type(&method, &lifetime)
                .to_token_stream()
                .to_string(),
            expected.to_token_stream().to_string()
        );
    }

    #[test]
    fn output_lifetimes_preserve_fn_trait_elision() {
        let method: ImplItemFn = parse_quote!(
            fn example<'call>(&'call self) -> &'call dyn Fn(&str) -> &str {
                unreachable!()
            }
        );
        let lifetime = Lifetime::new("'backing", Span::call_site());
        let expected: Type = parse_quote!(&'backing dyn Fn(&str) -> &str);
        assert_eq!(
            output_type(&method, &lifetime)
                .to_token_stream()
                .to_string(),
            expected.to_token_stream().to_string()
        );
    }
}
