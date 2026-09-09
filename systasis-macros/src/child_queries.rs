//! Resolve queries against independently built child metadata.
use crate::{child::ChildRegistration, parse::InterfaceGroup};
use quote::{format_ident, quote};
use syn::{
    ext::IdentExt,
    parse::Parser,
    visit_mut::{self, VisitMut},
    *,
};

pub(crate) struct Queries<'a> {
    pub children: &'a [ChildRegistration],
    pub error: Option<Error>,
    pub borrowed: Vec<(usize, Type)>,
    pub calls: Vec<Call>,
}

#[derive(Clone)]
pub(crate) struct Call {
    pub child: Type,
    pub path: Type,
    pub key: Type,
    pub operation: Ident,
    pub unchecked: bool,
}

/// Preserve declaration-site policy for an exact child value-type projection.
/// Wrappers around the projected type must undergo their own Copy selection.
pub(crate) fn projected_policy(ty: &Type, local: bool) -> Option<proc_macro2::TokenStream> {
    let Type::Path(ty) = ty else {
        return None;
    };
    let qualified = ty.qself.as_ref()?;
    let segments = &ty.path.segments;
    if segments.len() != 4
        || segments[0].ident != "systasis"
        || segments[1].ident != "scoped"
        || segments[2].ident != "Registered"
        || segments[3].ident != "Value"
    {
        return None;
    }
    let PathArguments::AngleBracketed(arguments) = &segments[2].arguments else {
        return None;
    };
    let mut arguments = arguments.args.iter();
    let path = arguments.next()?;
    let key = arguments.next()?;
    if arguments.next().is_some() {
        return None;
    }
    let owner = &qualified.ty;
    Some(quote!(<#owner as ::systasis::scoped::RegistrationPolicy<#path, #key, #local>>::Policy))
}
impl Queries<'_> {
    fn target(&mut self, mac: &Macro) -> Option<(usize, InterfaceGroup, bool, Type)> {
        let name = mac.path.get_ident()?.to_string();
        if !name.ends_with("_from") {
            return None;
        }
        let parsed = (|input: parse::ParseStream<'_>| {
            let dynamic = input.parse::<Option<Token![dyn]>>()?.is_some();
            let interface: InterfaceGroup = input.parse()?;
            input.parse::<Token![,]>()?;
            let path: Path = input.parse()?;
            Ok((interface, path, dynamic))
        })
        .parse2(mac.tokens.clone());
        let (interface, path, dynamic) = match parsed {
            Ok(value) => value,
            Err(error) => {
                self.error = Some(error);
                return None;
            }
        };
        let first = &path.segments.first()?.ident;
        let index = self
            .children
            .iter()
            .position(|child| child.name.unraw() == first.unraw())?;
        if path.leading_colon.is_some()
            || path
                .segments
                .iter()
                .any(|segment| !matches!(segment.arguments, PathArguments::None))
        {
            self.error = Some(Error::new_spanned(
                path,
                "child lookup paths contain only child or namespace names",
            ));
            return None;
        }
        let rest = path.segments.iter().skip(1).rev().fold(
            parse_quote!(::systasis::scoped::Here),
            |rest: Type, segment| {
                let key = crate::scopegen::key(&segment.ident.unraw().to_string());
                parse_quote!(::systasis::scoped::There<#key, #rest>)
            },
        );
        Some((index, interface, dynamic, rest))
    }
}

impl VisitMut for Queries<'_> {
    fn visit_expr_mut(&mut self, expression: &mut Expr) {
        if let Expr::Macro(query) = expression {
            let name = query
                .mac
                .path
                .get_ident()
                .map(ToString::to_string)
                .unwrap_or_default();
            let operation = match name.as_str() {
                "resolve_from" => "Owned",
                "try_resolve_from" => "TryOwned",
                "resolve_ref_from" => "Shared",
                "try_resolve_ref_from" => "TryShared",
                "try_resolve_ref_mut_from" => "TryExclusive",
                "resolve_clone_from" => "CloneValue",
                "try_resolve_clone_from" => "TryCloneValue",
                "resolve_dyn_ref_from" => "DynShared",
                "try_resolve_dyn_ref_from" => "TryDynShared",
                "resolve_unchecked_from" => "UncheckedOwned",
                "resolve_ref_unchecked_from" => "UncheckedShared",
                "resolve_ref_mut_unchecked_from" => "UncheckedExclusive",
                _ => {
                    visit_mut::visit_expr_mut(self, expression);
                    return;
                }
            };
            if let Some((index, interface, _, path)) = self.target(&query.mac) {
                let key = crate::scopegen::key(&interface.key());
                if matches!(
                    operation,
                    "Shared"
                        | "TryShared"
                        | "TryExclusive"
                        | "DynShared"
                        | "TryDynShared"
                        | "UncheckedShared"
                        | "UncheckedExclusive"
                ) {
                    let child = &self.children[index].ty.elem;
                    self.borrowed.push((
                        index,
                        parse_quote!(<#child as ::systasis::scoped::Borrowed<#path, #key>>::Mask),
                    ));
                }
                let position = Index::from(index);
                let operation = format_ident!("{operation}");
                self.calls.push(Call {
                    child: crate::scopegen::key(&self.children[index].name.unraw().to_string()),
                    path: path.clone(),
                    key: key.clone(),
                    operation: operation.clone(),
                    unchecked: name.contains("unchecked"),
                });
                let dispatch = if name.contains("unchecked") {
                    format_ident!("UnsafeResolve")
                } else {
                    format_ident!("Resolve")
                };
                *expression = parse_quote!(::systasis::scoped::#dispatch::<#path, #key, ::systasis::scoped::op::#operation>::resolve(&__systasis_children.#position));
                return;
            }
        }
        visit_mut::visit_expr_mut(self, expression);
    }
    fn visit_type_mut(&mut self, ty: &mut Type) {
        if let Type::Macro(query) = ty
            && query.mac.path.is_ident("resolve_type_from")
            && let Some((index, interface, dynamic, path)) = self.target(&query.mac)
        {
            let key = crate::scopegen::key(&interface.key());
            let child = &self.children[index].ty.elem;
            let target = if dynamic {
                quote!(<#child as ::systasis::scoped::DynRegistered<#path, #key>>::Target<'_>)
            } else {
                quote!(<#child as ::systasis::scoped::Registered<#path, #key>>::Value<'_>)
            };
            *ty = parse_quote!(#target);
            return;
        }
        visit_mut::visit_type_mut(self, ty);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_exact_value_projections_inherit_child_policy() {
        let projected: Type =
            parse_quote!(<Child<T> as ::systasis::scoped::Registered<Path, Key>>::Value<'_>);
        let selection = projected_policy(&projected, true).unwrap();
        let expected =
            quote!(<Child<T> as ::systasis::scoped::RegistrationPolicy<Path, Key, true>>::Policy);
        assert_eq!(selection.to_string(), expected.to_string());
        for other in [
            parse_quote!(Option<#projected>),
            parse_quote!(&#projected),
            parse_quote!(<Child<T> as ::systasis::scoped::DynRegistered<Path, Key>>::Target<'_>),
        ] {
            assert!(projected_policy(&other, false).is_none());
        }
    }

    #[test]
    fn child_paths_do_not_silently_discard_generic_arguments_or_roots() {
        let children = [syn::parse_str("primary: &Child").unwrap()];
        for source in [
            "resolve_from!(IValue, primary::<u32>)",
            "resolve_from!(IValue, ::primary)",
        ] {
            let mut expression: Expr = syn::parse_str(source).unwrap();
            let mut queries = Queries {
                children: &children,
                error: None,
                borrowed: Vec::new(),
                calls: Vec::new(),
            };
            queries.visit_expr_mut(&mut expression);
            assert_eq!(
                queries.error.unwrap().to_string(),
                "child lookup paths contain only child or namespace names"
            );
        }
    }
}
