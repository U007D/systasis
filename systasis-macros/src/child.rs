//! Parsing for named, independently owned child containers.

use std::collections::BTreeMap;
use syn::{
    Error, Ident, Result, Token, TypeReference,
    ext::IdentExt,
    parse::{Parse, ParseStream},
};

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
