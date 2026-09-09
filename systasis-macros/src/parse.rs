use quote::{ToTokens, quote};
use syn::{
    parse::{Parse, ParseStream},
    *,
};

/// A registration's complete, order-independent interface identity.
#[derive(Clone)]
pub(crate) struct InterfaceGroup(pub(crate) Vec<Path>);

impl Parse for InterfaceGroup {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        let mut paths = vec![input.parse::<Path>()?];
        while input.peek(Token![+]) {
            input.parse::<Token![+]>()?;
            paths.push(input.parse()?);
        }
        paths.sort_by_cached_key(|path| path.to_token_stream().to_string());
        paths.dedup_by(|left, right| {
            left.to_token_stream().to_string() == right.to_token_stream().to_string()
        });
        Ok(Self(paths))
    }
}

impl ToTokens for InterfaceGroup {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        let paths = &self.0;
        tokens.extend(quote!(#(#paths)+*));
    }
}

pub(crate) struct Registration {
    pub(crate) fresh: bool,
    pub(crate) constructor: Option<ExprClosure>,
    pub(crate) fallible: bool,
    pub(crate) dynamic: bool,
    pub(crate) value: Expr,
    pub(crate) ty: Type,
    pub(crate) interface: InterfaceGroup,
}
impl Parse for Registration {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        let value = input.parse()?;
        input.parse::<Token![:]>()?;
        let ty = input.parse()?;
        input.parse::<Token![as]>()?;
        let dynamic = input.parse::<Option<Token![dyn]>>()?.is_some();
        let interface: InterfaceGroup = input.parse()?;
        if dynamic && interface.0.len() > 1 {
            return Err(Error::new_spanned(
                &interface,
                "combined dyn trait accessors are not implemented yet",
            ));
        }
        Ok(Self {
            fresh: false,
            constructor: None,
            fallible: false,
            dynamic,
            value,
            ty,
            interface,
        })
    }
}
pub(crate) struct Registrations(pub(crate) Vec<Registration>);
impl Parse for Registrations {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        let mut entries = Vec::new();
        while !input.is_empty() {
            let name: Ident = input.parse()?;
            if name != "register_value" && name != "register_type" && name != "register_type_with" {
                return Err(Error::new_spanned(
                    name,
                    "expected register_value!, register_type!, or register_type_with!",
                ));
            }
            input.parse::<Token![!]>()?;
            let body;
            parenthesized!(body in input);
            entries.push(if name == "register_type" || name == "register_type_with" {
                let ty = body.parse()?;
                body.parse::<Token![as]>()?;
                let interface = body.parse()?;
                let mut fallible = false;
                let constructor = if name == "register_type_with" {
                    body.parse::<Token![,]>()?;
                    fallible = body.parse::<Option<Token![try]>>()?.is_some();
                    let closure: ExprClosure = body.parse()?;
                    if !closure.inputs.is_empty() || closure.asyncness.is_some() {
                        return Err(Error::new_spanned(
                            &closure,
                            "constructor must be a synchronous zero-argument closure",
                        ));
                    }
                    if fallible && matches!(closure.output, ReturnType::Default) {
                        return Err(Error::new_spanned(
                            &closure,
                            "fallible constructor requires an explicit return type",
                        ));
                    }
                    Some(closure)
                } else {
                    None
                };
                Registration {
                    fresh: true,
                    constructor,
                    fallible,
                    dynamic: false,
                    value: parse_quote!(()),
                    ty,
                    interface,
                }
            } else {
                body.parse()?
            });
            if !body.is_empty() {
                return Err(body.error("unexpected registration tokens"));
            }
            input.parse::<Token![;]>()?;
        }
        Ok(Self(entries))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn group_identity_normalizes_order_without_erasing_paths_or_arguments() {
        let key = |source: &str| {
            syn::parse_str::<InterfaceGroup>(source)
                .unwrap()
                .to_token_stream()
                .to_string()
        };
        assert_eq!(
            key("b::IWrite + a::IRead<u8>"),
            key("a::IRead<u8> + b::IWrite")
        );
        assert_ne!(key("a::IRead<u8>"), key("a::IRead<u16>"));
        assert_ne!(key("a::IRead<u8>"), key("b::IRead<u8>"));
        assert_ne!(key("a::IRead<u8>"), key("a::IRead<u8> + b::IWrite"));
    }
}
