//! Shared build-time and resolution-time constructor wiring.

use crate::parse::Registration;
use proc_macro2::Span;
use quote::format_ident;
use std::collections::{BTreeMap, BTreeSet};
use syn::{Expr, parse_quote};

/// Separate build-time storage from caller locals. Constructor queries use
/// `self._children` because glob imports can shadow even mixed-site bindings.
pub(crate) fn children_ident() -> syn::Ident {
    format_ident!("__systasis_children", span = Span::mixed_site())
}

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
    _local_policy: bool,
    turbofish: &Option<proc_macro2::TokenStream>,
    _dynamic: &[Option<syn::Type>],
) -> BTreeMap<(usize, String), Expr> {
    let children = children_ident();
    let slot = |index| {
        let name = format_ident!("__systasis_slot_{index}", span = Span::mixed_site());
        if building {
            parse_quote!(#name.as_ref().unwrap_or_else(|| ::core::unreachable!("dependency layer completed before this initializer")))
        } else {
            parse_quote!(self.#name)
        }
    };
    let mut result = BTreeMap::new();
    for (index, registration) in registrations.iter().enumerate() {
        if registration.constructor.is_some() {
            let function = format_ident!("__systasis_construct_{index}");
            let arguments: Vec<Expr> = arguments(index, dependencies)
                .into_iter()
                .map(slot)
                .collect();
            let method = if registration.fallible {
                "try_resolve"
            } else {
                "resolve"
            };
            let call: Expr = if building {
                parse_quote!(self::__systasis_injected::#function #turbofish (#(#arguments,)* &#children))
            } else {
                parse_quote!(self::__systasis_injected::#function #turbofish (#(#arguments,)* self._children))
            };
            result.insert((index, method.into()), call.clone());
            if !registration.fallible {
                result.insert(
                    (index, "try_resolve".into()),
                    parse_quote!(
                        ::core::result::Result::<_, ::systasis::__private::Never>::Ok(#call)
                    ),
                );
            }
        } else {
            for method in [
                "resolve",
                "try_resolve",
                "resolve_clone",
                "try_resolve_clone",
            ] {
                let method_ident = format_ident!("{method}");
                let owner: Expr = slot(index);
                result.insert((index, method.into()), parse_quote!(#owner.#method_ident()));
            }
            if cfg!(feature = "resolve_unchecked") && !registration.fresh {
                for method in ["resolve_unchecked"] {
                    let method_ident = format_ident!("{method}");
                    let owner: Expr = slot(index);
                    // Deliberately no unsafe block: the user's query must be
                    // inside an explicit unsafe context.
                    result.insert((index, method.into()), parse_quote!(#owner.#method_ident()));
                }
            }
        }
    }
    result
}
