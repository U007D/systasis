//! Shared build-time and resolution-time constructor wiring.

use crate::parse::Registration;
use proc_macro2::Span;
use quote::format_ident;
use std::collections::{BTreeMap, BTreeSet};
use syn::{Expr, parse_quote};

pub(crate) fn transitive(
    dependencies: &[BTreeSet<usize>],
    order: &[usize],
) -> Vec<BTreeSet<usize>> {
    let mut all = dependencies.to_vec();
    for &index in order {
        for &dependency in &dependencies[index] {
            let inherited = all[dependency].clone();
            all[index].extend(inherited);
        }
    }
    all
}

pub(crate) fn arguments(index: usize, dependencies: &[BTreeSet<usize>]) -> Vec<usize> {
    std::iter::once(index)
        .chain(dependencies[index].iter().copied())
        .collect()
}

pub(crate) fn replacements(
    registrations: &[Registration],
    dependencies: &[BTreeSet<usize>],
    building: bool,
    local_policy: bool,
    turbofish: &Option<proc_macro2::TokenStream>,
) -> BTreeMap<(usize, String), Expr> {
    let slot = |index| {
        let name = format_ident!("__systasis_slot_{index}", span = Span::mixed_site());
        if building {
            parse_quote!(#name.as_ref().unwrap_or_else(|| ::core::unreachable!("dependency layer completed before this initializer")))
        } else {
            parse_quote!(#name)
        }
    };
    let mut result = BTreeMap::new();
    for (index, registration) in registrations.iter().enumerate() {
        if registration.constructor.is_some() {
            let function = format_ident!("construct_{index}");
            let arguments: Vec<Expr> = arguments(index, dependencies)
                .into_iter()
                .map(slot)
                .collect();
            let method = if registration.fallible {
                "try_resolve"
            } else {
                "resolve"
            };
            result.insert(
                (index, method.into()),
                parse_quote!(__systasis_injected::#function #turbofish (#(#arguments),*)),
            );
        } else {
            if registration.dynamic {
                let owner: Expr = slot(index);
                let interface = &registration.interface;
                let guard: syn::Path = if local_policy {
                    parse_quote!(::core::cell::Ref)
                } else {
                    parse_quote!(::systasis::__private::Ref)
                };
                result.insert(
                    (index, "resolve_dyn_ref".into()),
                    parse_quote!(
                        (#owner.resolve_ref() as &(dyn #interface + '_))
                    ),
                );
                result.insert((index, "try_resolve_dyn_ref".into()), parse_quote!(
                    #owner.try_resolve_ref().map(|guard| #guard::map(guard, |value| value as &(dyn #interface + '_)))
                ));
            }
            for method in [
                "resolve",
                "try_resolve",
                "resolve_ref",
                "try_resolve_ref",
                "try_resolve_ref_mut",
                "resolve_clone",
                "try_resolve_clone",
            ] {
                let method_ident = format_ident!("{method}");
                let owner: Expr = slot(index);
                result.insert((index, method.into()), parse_quote!(#owner.#method_ident()));
            }
        }
    }
    result
}
