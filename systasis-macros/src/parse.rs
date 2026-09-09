use quote::{ToTokens, quote};
use syn::{
    ext::IdentExt,
    parse::{Parse, ParseStream},
    *,
};

/// The omitted namespace and explicit `default` identify the same registrations.
#[derive(Clone, Default)]
pub(crate) struct Namespace(pub(crate) Option<Ident>);

impl Namespace {
    fn named(name: Ident) -> Self {
        Self((name.unraw() != "default").then_some(name))
    }

    fn registration(input: ParseStream<'_>) -> Result<Self> {
        if input.parse::<Option<Token![in]>>()?.is_some() {
            Ok(Self::named(input.parse()?))
        } else {
            Ok(Self::default())
        }
    }

    pub(crate) fn query(input: ParseStream<'_>, from: bool) -> Result<Self> {
        if from {
            input.parse::<Token![,]>()?;
            Ok(Self::named(input.parse()?))
        } else {
            Ok(Self::default())
        }
    }

    pub(crate) fn key(&self, interface: &InterfaceGroup) -> String {
        self.0.as_ref().map_or_else(
            || interface.to_token_stream().to_string(),
            |namespace| format!("{} in {}", interface.to_token_stream(), namespace.unraw()),
        )
    }
}

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
    pub(crate) namespace: Namespace,
}
impl Parse for Registration {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        let value = input.parse()?;
        input.parse::<Token![:]>()?;
        let ty = input.parse()?;
        input.parse::<Token![as]>()?;
        let dynamic = input.parse::<Option<Token![dyn]>>()?.is_some();
        let interface: InterfaceGroup = input.parse()?;
        let namespace = Namespace::registration(input)?;
        Ok(Self {
            fresh: false,
            constructor: None,
            fallible: false,
            dynamic,
            value,
            ty,
            interface,
            namespace,
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
                let namespace = Namespace::registration(&body)?;
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
                    namespace,
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
