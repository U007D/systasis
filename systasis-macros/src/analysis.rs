use crate::parse::InterfaceGroup;
use quote::ToTokens;
use std::collections::{BTreeMap, BTreeSet};
use syn::{
    parse::Parser,
    visit_mut::{self, VisitMut},
    *,
};
pub(crate) struct Queries<'a> {
    pub(crate) indices: &'a BTreeMap<String, usize>,
    pub(crate) dependencies: BTreeSet<usize>,
    pub(crate) borrowed: BTreeSet<usize>,
    pub(crate) error: Option<Error>,
    pub(crate) replacements: Option<&'a BTreeMap<(usize, String), Expr>>,
}
impl VisitMut for Queries<'_> {
    fn visit_expr_mut(&mut self, expression: &mut Expr) {
        if let Expr::Macro(query) = expression
            && [
                "try_resolve",
                "resolve",
                "try_resolve_ref",
                "try_resolve_ref_mut",
                "try_resolve_clone",
                "resolve_ref",
                "resolve_clone",
                "resolve_dyn_ref",
                "try_resolve_dyn_ref",
            ]
            .iter()
            .any(|name| query.mac.path.is_ident(name))
        {
            let interface = match syn::parse2::<InterfaceGroup>(query.mac.tokens.clone()) {
                Ok(interface) => interface,
                Err(error) => {
                    self.error = Some(error);
                    return;
                }
            };
            let key = interface.to_token_stream().to_string();
            if let Some(index) = self.indices.get(&key) {
                self.dependencies.insert(*index);

                let method =
                    query.mac.path.get_ident().unwrap_or_else(|| {
                        unreachable!("query path was checked to be an identifier")
                    });
                if [
                    "resolve_ref",
                    "try_resolve_ref",
                    "try_resolve_ref_mut",
                    "resolve_dyn_ref",
                    "try_resolve_dyn_ref",
                ]
                .iter()
                .any(|name| method == name)
                {
                    self.borrowed.insert(*index);
                }
                if let Some(replacements) = self.replacements {
                    if let Some(replacement) = replacements.get(&(*index, method.to_string())) {
                        *expression = replacement.clone();
                    } else {
                        self.error = Some(Error::new_spanned(
                            query,
                            "requested resolver is unavailable for this constructor",
                        ));
                    }
                }
            } else {
                self.error = Some(Error::new_spanned(query, "unregistered dependency"));
            }
        } else {
            visit_mut::visit_expr_mut(self, expression);
        }
    }
}

pub(crate) struct TypeLookup<'a> {
    pub(crate) indices: &'a BTreeMap<String, usize>,
    pub(crate) types: &'a [Type],
    pub(crate) dynamic: &'a [Option<Type>],
    pub(crate) active: Vec<usize>,
    pub(crate) dependencies: BTreeSet<usize>,
    pub(crate) error: Option<Error>,
}
impl VisitMut for TypeLookup<'_> {
    fn visit_type_mut(&mut self, ty: &mut Type) {
        if let Type::Macro(query) = ty
            && (query.mac.path.is_ident("registered_type")
                || query.mac.path.is_ident("resolve_type"))
        {
            let parsed = (|input: syn::parse::ParseStream<'_>| {
                let dynamic = input.parse::<Option<Token![dyn]>>()?.is_some();
                let interface = input.parse::<InterfaceGroup>()?;
                Ok((dynamic, interface))
            })
            .parse2(query.mac.tokens.clone());
            let (dynamic, interface) = match parsed {
                Ok(parsed) => parsed,
                Err(error) => {
                    self.error = Some(error);
                    return;
                }
            };
            let key = interface.to_token_stream().to_string();
            let Some(&index) = self.indices.get(&key) else {
                self.error = Some(Error::new_spanned(query, "unregistered type lookup"));
                return;
            };
            if dynamic {
                let Some(target) = &self.dynamic[index] else {
                    self.error = Some(Error::new_spanned(
                        query,
                        "dyn type lookup requires an as dyn registration",
                    ));
                    return;
                };
                self.dependencies.insert(index);
                *ty = parse_quote!((#target));
                return;
            }
            if self.active.contains(&index) {
                self.error = Some(Error::new_spanned(query, "registered type lookup cycle"));
                return;
            }
            self.active.push(index);
            self.dependencies.insert(index);
            *ty = self.types[index].clone();
            self.visit_type_mut(ty);
            self.active.pop();
        } else {
            visit_mut::visit_type_mut(self, ty);
        }
    }
}
