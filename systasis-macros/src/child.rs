//! Parsing for named, independently owned child containers.

use quote::quote;
use std::collections::BTreeMap;
use syn::{
    Error, Ident, Result, Token, TypeReference,
    ext::IdentExt,
    parse::{Parse, ParseStream},
};

pub(crate) struct Input {
    pub(crate) registrations: crate::parse::Registrations,
    pub(crate) children: Vec<ChildRegistration>,
}

/// A child alias must not acquire lifetimes belonging only to sibling children.
pub(crate) fn scope_generics(generics: &syn::Generics, ty: &syn::Type) -> syn::Generics {
    use std::collections::BTreeSet;
    use syn::visit_mut::{self, VisitMut};

    #[derive(Default)]
    struct References {
        paths: BTreeSet<String>,
        lifetimes: BTreeSet<String>,
        bound_lifetimes: BTreeSet<String>,
    }
    impl References {
        fn contains(&self, parameter: &syn::GenericParam) -> bool {
            match parameter {
                syn::GenericParam::Type(p) => self.paths.contains(&p.ident.to_string()),
                syn::GenericParam::Const(p) => self.paths.contains(&p.ident.to_string()),
                syn::GenericParam::Lifetime(p) => {
                    self.lifetimes.contains(&p.lifetime.ident.to_string())
                }
            }
        }
        fn bind(&mut self, lifetimes: &Option<syn::BoundLifetimes>) -> BTreeSet<String> {
            let previous = self.bound_lifetimes.clone();
            if let Some(lifetimes) = lifetimes {
                self.bound_lifetimes
                    .extend(lifetimes.lifetimes.iter().filter_map(|p| {
                        if let syn::GenericParam::Lifetime(p) = p {
                            Some(p.lifetime.ident.to_string())
                        } else {
                            None
                        }
                    }));
            }
            previous
        }
    }
    impl VisitMut for References {
        fn visit_path_mut(&mut self, path: &mut syn::Path) {
            if path.leading_colon.is_none()
                && let Some(first) = path.segments.first()
            {
                self.paths.insert(first.ident.to_string());
            }
            visit_mut::visit_path_mut(self, path);
        }
        fn visit_lifetime_mut(&mut self, lifetime: &mut syn::Lifetime) {
            if !self.bound_lifetimes.contains(&lifetime.ident.to_string()) {
                self.lifetimes.insert(lifetime.ident.to_string());
            }
        }
        fn visit_trait_bound_mut(&mut self, bound: &mut syn::TraitBound) {
            let previous = self.bind(&bound.lifetimes);
            visit_mut::visit_trait_bound_mut(self, bound);
            self.bound_lifetimes = previous;
        }
        fn visit_type_fn_ptr_mut(&mut self, ty: &mut syn::TypeFnPtr) {
            let previous = self.bind(&ty.lifetimes);
            visit_mut::visit_type_fn_ptr_mut(self, ty);
            self.bound_lifetimes = previous;
        }
        fn visit_predicate_type_mut(&mut self, predicate: &mut syn::PredicateType) {
            let previous = self.bind(&predicate.lifetimes);
            visit_mut::visit_predicate_type_mut(self, predicate);
            self.bound_lifetimes = previous;
        }
    }

    let mut references = References::default();
    references.visit_type_mut(&mut ty.clone());
    let mut selected = generics.clone();
    selected.params = generics
        .params
        .iter()
        .filter(|p| references.contains(p))
        .cloned()
        .collect();
    let removed = generics
        .params
        .iter()
        .filter(|p| !references.contains(p))
        .collect::<Vec<_>>();
    // Function-only relationships must not leave free parameters in the alias.
    // Keep independent bounds (notably Copy/?Sized) on each retained parameter.
    for parameter in &mut selected.params {
        match parameter {
            syn::GenericParam::Type(parameter) => {
                parameter.bounds = parameter
                    .bounds
                    .iter()
                    .filter(|bound| {
                        let mut mentioned = References::default();
                        mentioned.visit_type_param_bound_mut(&mut (*bound).clone());
                        !removed.iter().any(|p| mentioned.contains(p))
                    })
                    .cloned()
                    .collect();
                if parameter.bounds.is_empty() {
                    parameter.colon_token = None;
                }
                if let Some((_, default)) = &parameter.default {
                    let mut mentioned = References::default();
                    mentioned.visit_type_mut(&mut default.clone());
                    if removed.iter().any(|p| mentioned.contains(p)) {
                        parameter.default = None;
                    }
                }
            }
            syn::GenericParam::Lifetime(parameter) => {
                parameter.bounds = parameter
                    .bounds
                    .iter()
                    .filter(|bound| {
                        let mut mentioned = References::default();
                        mentioned.visit_lifetime_mut(&mut (*bound).clone());
                        !removed.iter().any(|p| mentioned.contains(p))
                    })
                    .cloned()
                    .collect();
                if parameter.bounds.is_empty() {
                    parameter.colon_token = None;
                }
            }
            syn::GenericParam::Const(parameter) => {
                if let Some((_, default)) = &parameter.default {
                    let mut mentioned = References::default();
                    mentioned.visit_expr_mut(&mut default.clone());
                    if removed.iter().any(|p| mentioned.contains(p)) {
                        parameter.default = None;
                    }
                }
            }
        }
    }
    // Original predicates can mention parameters absent from this child's type.
    // Keep only predicates whose generic parameters all remain selected.
    if let Some(clause) = &mut selected.where_clause {
        clause.predicates = clause
            .predicates
            .iter()
            .filter_map(|predicate| {
                // Bounds in one predicate are independent: retain T: Copy even
                // when T: Copy + Related<U> also contains a parent-only U.
                let mut predicate = predicate.clone();
                if let syn::WherePredicate::Type(ty) = &mut predicate {
                    ty.bounds = ty
                        .bounds
                        .iter()
                        .filter(|bound| {
                            let mut mentioned = References::default();
                            mentioned.bind(&ty.lifetimes);
                            mentioned.visit_type_param_bound_mut(&mut (*bound).clone());
                            !removed.iter().any(|p| mentioned.contains(p))
                        })
                        .cloned()
                        .collect();
                    if ty.bounds.is_empty() {
                        return None;
                    }
                }
                let mut mentioned = References::default();
                mentioned.visit_where_predicate_mut(&mut predicate);
                (!removed.iter().any(|p| mentioned.contains(p))).then_some(predicate)
            })
            .collect();
        if clause.predicates.is_empty() {
            selected.where_clause = None;
        }
    }
    selected
}
impl Parse for Input {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        let mut declarations = proc_macro2::TokenStream::new();
        let mut children = Vec::new();
        while !input.is_empty() {
            let name: Ident = input.parse()?;
            input.parse::<Token![!]>()?;
            let body;
            syn::parenthesized!(body in input);
            let tokens: proc_macro2::TokenStream = body.parse()?;
            input.parse::<Token![;]>()?;
            if name == "register_container" {
                children.push(syn::parse2(tokens)?);
            } else {
                declarations.extend(quote!(#name!(#tokens);));
            }
        }
        validate_names(&children)?;
        Ok(Self {
            registrations: syn::parse2(declarations)?,
            children,
        })
    }
}

/// The supplied binding is already a shared reference to a child container.
pub(crate) struct ChildRegistration {
    pub(crate) name: Ident,
    pub(crate) ty: TypeReference,
}

impl Parse for ChildRegistration {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        let name: Ident = input.parse()?;
        if name.unraw() == "default" {
            return Err(Error::new_spanned(
                name,
                "a child container requires a non-default path name; default namespaces cannot be flattened",
            ));
        }
        input.parse::<Token![:]>()?;
        let ty: TypeReference = input.parse()?;
        if let Some(mutability) = ty.mutability {
            return Err(Error::new_spanned(
                mutability,
                "register a child container by shared reference",
            ));
        }
        Ok(Self { name, ty })
    }
}

/// Unlike same-interface value overrides, duplicate child names are errors.
pub(crate) fn validate_names(children: &[ChildRegistration]) -> Result<()> {
    let mut seen = BTreeMap::new();
    for child in children {
        if let Some(previous) = seen.insert(child.name.unraw().to_string(), &child.name) {
            let mut error = Error::new_spanned(&child.name, "duplicate child container path");
            error.combine(Error::new_spanned(previous, "first child with this path"));
            return Err(error);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use quote::ToTokens;

    fn selected(source: &str, ty: syn::Type) -> syn::Generics {
        let function: syn::ItemFn = syn::parse_str(source).unwrap();
        scope_generics(&function.sig.generics, &ty)
    }

    #[test]
    fn function_only_bounds_are_removed_individually() {
        let selected = selected(
            "fn build<'a: 'b, 'b, T: Copy + Related<U>, U>() where T: Clone + Related<U> {}",
            syn::parse_quote!(&'a Child<T>),
        );
        assert_eq!(
            selected.params.to_token_stream().to_string(),
            "'a , T : Copy"
        );
        assert_eq!(
            selected.where_clause.unwrap().to_token_stream().to_string(),
            "where T : Clone"
        );
    }

    #[test]
    fn defaults_do_not_leave_removed_parameters_in_aliases() {
        let selected = selected(
            "fn build<T = U, U = u8, const M: usize = 2, const N: usize = M>() {}",
            syn::parse_quote!(Child<T, N>),
        );
        assert_eq!(
            selected.params.to_token_stream().to_string(),
            "T , const N : usize"
        );
    }

    #[test]
    fn scope_omits_sibling_lifetimes_and_keeps_relevant_bounds() {
        let selected = selected(
            "fn build<'a, 'sibling, T: Clone, U, const N: usize>() where T: Send, U: Sync {}",
            syn::parse_quote!(&'a Child<T, N>),
        );
        assert_eq!(
            selected.params.to_token_stream().to_string(),
            "'a , T : Clone , const N : usize"
        );
        assert_eq!(
            selected.where_clause.unwrap().to_token_stream().to_string(),
            "where T : Send"
        );
    }

    #[test]
    fn associated_names_and_lifetime_names_are_not_type_parameters() {
        let selected = selected(
            "fn build<'Item, Item, T>() where T: Iterator<Item = u8>, T: 'Item {}",
            syn::parse_quote!(Child<T>),
        );
        assert_eq!(selected.params.to_token_stream().to_string(), "T");
        assert_eq!(
            selected.where_clause.unwrap().to_token_stream().to_string(),
            "where T : Iterator < Item = u8 >"
        );
    }

    #[test]
    fn higher_ranked_lifetimes_do_not_select_outer_parameters() {
        let selected = selected(
            "fn build<'a, T>() where T: for<'a> Fn(&'a str) {}",
            syn::parse_quote!(Child<T, for<'a> fn(&'a str)>),
        );
        assert_eq!(selected.params.to_token_stream().to_string(), "T");
        assert_eq!(
            selected.where_clause.unwrap().to_token_stream().to_string(),
            "where T : for < 'a > Fn (& 'a str)"
        );
    }

    #[test]
    fn qualified_projections_and_const_expressions_select_parameters() {
        let selected = selected(
            "fn build<'unused, T, const N: usize>() where T: Iterator {}",
            syn::parse_quote!(Child<<T as Iterator>::Item, {N + 1}>),
        );
        assert_eq!(
            selected.params.to_token_stream().to_string(),
            "T , const N : usize"
        );
        assert!(selected.where_clause.is_some());
    }

    #[test]
    fn shared_generic_alias_retains_the_authored_type_and_lifetime() {
        let child: ChildRegistration =
            syn::parse_str("primary: &'a database::Alias<T, 4>").unwrap();
        assert_eq!(child.name, "primary");
        assert_eq!(
            child.ty.to_token_stream().to_string(),
            "& 'a database :: Alias < T , 4 >"
        );
    }

    #[test]
    fn composition_requires_named_shared_references() {
        for source in [
            "default: &Container",
            "r#default: &Container",
            "primary: &mut Container",
            "primary: Container",
            "&Container",
        ] {
            assert!(
                syn::parse_str::<ChildRegistration>(source).is_err(),
                "{source}"
            );
        }
    }

    #[test]
    fn duplicate_paths_are_not_overrides_and_raw_spelling_is_normalized() {
        let children = [
            syn::parse_str("primary: &First").unwrap(),
            syn::parse_str("r#primary: &Second").unwrap(),
        ];
        assert!(
            validate_names(&children)
                .unwrap_err()
                .to_string()
                .contains("duplicate child")
        );
        let children = [
            syn::parse_str("primary: &Alias").unwrap(),
            syn::parse_str("replica: &Alias").unwrap(),
        ];
        assert!(validate_names(&children).is_ok());
    }
}
