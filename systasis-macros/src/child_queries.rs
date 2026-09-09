//! Resolve queries against independently built child metadata.
use crate::{child::ChildRegistration, parse::InterfaceGroup};
use quote::{format_ident, quote};
use syn::{
    parse::Parser,
    visit_mut::{self, VisitMut},
    *,
};

pub(crate) struct Queries<'a> {
    pub children: &'a [ChildRegistration],
    pub error: Option<Error>,
    pub borrowed: Vec<(usize, Type)>,
}
impl Queries<'_> {
    fn target(&mut self, mac: &Macro) -> Option<(usize, InterfaceGroup, bool)> {
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
            .position(|child| child.name == *first)?;
        if path.segments.len() != 1 {
            self.error = Some(Error::new_spanned(
                path,
                "nested child queries are not yet integrated",
            ));
            return None;
        }
        Some((index, interface, dynamic))
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
            if let Some((index, interface, _)) = self.target(&query.mac) {
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
                    let namespace = crate::scopegen::key("default");
                    self.borrowed.push((
                        index,
                        parse_quote!(::systasis::scoped::key::Pair<#namespace, #key>),
                    ));
                }
                let position = Index::from(index);
                let operation = format_ident!("{operation}");
                let dispatch = if name.contains("unchecked") {
                    format_ident!("UnsafeResolve")
                } else {
                    format_ident!("Resolve")
                };
                *expression = parse_quote!(::systasis::scoped::#dispatch::<::systasis::scoped::Here, #key, ::systasis::scoped::op::#operation>::resolve(&__systasis_children.#position));
                return;
            }
        }
        visit_mut::visit_expr_mut(self, expression);
    }
    fn visit_type_mut(&mut self, ty: &mut Type) {
        if let Type::Macro(query) = ty
            && query.mac.path.is_ident("resolve_type_from")
            && let Some((index, interface, dynamic)) = self.target(&query.mac)
        {
            let key = crate::scopegen::key(&interface.key());
            let child = &self.children[index].ty.elem;
            let target = if dynamic {
                quote!(<#child as ::systasis::scoped::DynRegistered<::systasis::scoped::Here, #key>>::Target<'_>)
            } else {
                quote!(<#child as ::systasis::scoped::Registered<::systasis::scoped::Here, #key>>::Value<'_>)
            };
            *ty = parse_quote!(#target);
            return;
        }
        visit_mut::visit_type_mut(self, ty);
    }
}
