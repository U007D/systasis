//! Registration-site Copy policy for types involving enclosing generics.

use std::collections::BTreeSet;

use quote::ToTokens;
use syn::visit_mut::{self, VisitMut};
use syn::{Generics, Path, Type, TypeParamBound};

/// Whether the registered type mentions an enclosing type, const, or lifetime.
pub(crate) fn depends_on_generics(ty: &Type, generics: &Generics) -> bool {
    struct Dependencies {
        paths: BTreeSet<String>,
        lifetimes: BTreeSet<String>,
        found: bool,
    }

    impl VisitMut for Dependencies {
        fn visit_path_mut(&mut self, path: &mut Path) {
            self.found |= path.leading_colon.is_none()
                && path
                    .segments
                    .first()
                    .is_some_and(|segment| self.paths.contains(&segment.ident.to_string()));
            visit_mut::visit_path_mut(self, path);
        }

        fn visit_lifetime_mut(&mut self, lifetime: &mut syn::Lifetime) {
            self.found |= self.lifetimes.contains(&lifetime.ident.to_string());
        }
    }

    let mut dependencies = Dependencies {
        paths: generics
            .params
            .iter()
            .filter_map(|parameter| match parameter {
                syn::GenericParam::Type(parameter) => Some(parameter.ident.to_string()),
                syn::GenericParam::Const(parameter) => Some(parameter.ident.to_string()),
                syn::GenericParam::Lifetime(_) => None,
            })
            .collect(),
        lifetimes: generics
            .lifetimes()
            .map(|parameter| parameter.lifetime.ident.to_string())
            .collect(),
        found: false,
    };
    dependencies.visit_type_mut(&mut ty.clone());
    dependencies.found
}

/// Recognizes an explicit Copy bound on the entire registered type.
///
/// This is syntactic recognition, not trait solving: an argument's Copy bound
/// does not count as a bound on its wrapper. Renamed traits and differently
/// spelled equivalent types are not normalized here.
pub(crate) fn has_explicit_copy_bound(ty: &Type, generics: &Generics) -> bool {
    fn is_copy(bound: &TypeParamBound) -> bool {
        let TypeParamBound::Trait(bound) = bound else {
            return false;
        };
        bound.maybe.is_none()
            && bound.modifiers.require_empty().is_ok()
            && matches!(
                bound
                    .path
                    .to_token_stream()
                    .to_string()
                    .replace(' ', "")
                    .as_str(),
                "Copy"
                    | "core::marker::Copy"
                    | "::core::marker::Copy"
                    | "std::marker::Copy"
                    | "::std::marker::Copy"
            )
    }

    let registered = ty.to_token_stream().to_string();
    generics.type_params().any(|parameter| {
        parameter.ident.to_string() == registered && parameter.bounds.iter().any(is_copy)
    }) || generics.where_clause.as_ref().is_some_and(|clause| {
        clause.predicates.iter().any(|predicate| {
            let syn::WherePredicate::Type(predicate) = predicate else {
                return false;
            };
            predicate.bounded_ty.to_token_stream().to_string() == registered
                && predicate.bounds.iter().any(is_copy)
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use syn::parse_quote;

    fn signature(source: &str) -> Generics {
        syn::parse_str::<syn::ItemFn>(source).unwrap().sig.generics
    }

    #[test]
    fn detects_type_const_and_lifetime_arguments() {
        let generics = signature("fn configure<'a, T, const N: usize>() {}");
        for ty in [
            parse_quote!(T),
            parse_quote!(Wrapper<T>),
            parse_quote!(T::Item),
            parse_quote!(<T as Iterator>::Item),
            parse_quote!([u8; N]),
            parse_quote!([u8; { N + 1 }]),
            parse_quote!(&'a str),
            parse_quote!(fn(&'a T) -> [u8; N]),
        ] {
            assert!(
                depends_on_generics(&ty, &generics),
                "{}",
                ty.to_token_stream()
            );
        }
    }

    #[test]
    fn concrete_types_and_other_names_do_not_depend_on_generics() {
        let generics = signature("fn configure<'a, T, const N: usize>() {}");
        for ty in [
            parse_quote!(u32),
            parse_quote!(Wrapper<u32>),
            parse_quote!(&'static str),
            parse_quote!([u8; 4]),
            parse_quote!(module::T),
            parse_quote!(::T),
        ] {
            assert!(
                !depends_on_generics(&ty, &generics),
                "{}",
                ty.to_token_stream()
            );
        }
    }

    #[test]
    fn lifetime_and_path_names_are_separate() {
        assert!(!depends_on_generics(
            &parse_quote!(a),
            &signature("fn configure<'a>() {}"),
        ));
        assert!(!depends_on_generics(
            &parse_quote!(&'T str),
            &signature("fn configure<T>() {}"),
        ));
    }

    #[test]
    fn accepts_inline_and_qualified_where_bounds() {
        for source in [
            "fn configure<T: Copy>() {}",
            "fn configure<T: Clone + Copy>() {}",
            "fn configure<T>() where T: Copy {}",
            "fn configure<T>() where T: core::marker::Copy {}",
            "fn configure<T>() where T: ::core::marker::Copy {}",
            "fn configure<T>() where T: std::marker::Copy {}",
            "fn configure<T>() where T: ::std::marker::Copy {}",
        ] {
            assert!(
                has_explicit_copy_bound(&parse_quote!(T), &signature(source)),
                "{source}"
            );
        }
    }

    #[test]
    fn requires_bound_on_whole_registered_type() {
        let indirect = signature("fn configure<T: Copy>() {}");
        assert!(!has_explicit_copy_bound(
            &parse_quote!(Wrapper<T>),
            &indirect
        ));
        let explicit = signature("fn configure<T>() where Wrapper<T>: Copy {}");
        assert!(has_explicit_copy_bound(
            &parse_quote!(Wrapper<T>),
            &explicit
        ));
        assert!(!has_explicit_copy_bound(&parse_quote!(T), &explicit));
    }

    #[test]
    fn accepts_associated_array_and_borrowed_whole_type_bounds() {
        let generics = signature(
            "fn configure<'a, T: Iterator, const N: usize>()
             where T::Item: Copy, [T; N]: Copy, &'a T: Copy {}",
        );
        for ty in [
            parse_quote!(T::Item),
            parse_quote!([T; N]),
            parse_quote!(&'a T),
        ] {
            assert!(
                has_explicit_copy_bound(&ty, &generics),
                "{}",
                ty.to_token_stream()
            );
        }
    }

    #[test]
    fn does_not_infer_copy_from_other_traits_or_relaxed_bounds() {
        for source in [
            "fn configure<T>() {}",
            "fn configure<T: Clone>() {}",
            "fn configure<T: CopySupertrait>() {}",
            "fn configure<T: module::Copy>() {}",
            "fn configure<T: ?Copy>() {}",
        ] {
            assert!(
                !has_explicit_copy_bound(&parse_quote!(T), &signature(source)),
                "{source}"
            );
        }
    }
}
