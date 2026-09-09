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
            || interface.key(),
            |namespace| format!("{} in {}", interface.key(), namespace.unraw()),
        )
    }
}

/// A registration's complete, order-independent interface identity.
#[derive(Clone)]
pub(crate) struct InterfaceGroup(pub(crate) Vec<Path>);

/// Raw identifier notation does not change a Rust identifier's identity.
/// Normalize keys only, retaining original tokens when emitting trait bounds.
fn identifier_key(tokens: proc_macro2::TokenStream) -> String {
    fn normalize(tokens: proc_macro2::TokenStream) -> proc_macro2::TokenStream {
        tokens
            .into_iter()
            .map(|token| match token {
                proc_macro2::TokenTree::Ident(ident) => {
                    proc_macro2::TokenTree::Ident(ident.unraw())
                }
                proc_macro2::TokenTree::Group(group) => proc_macro2::TokenTree::Group(
                    proc_macro2::Group::new(group.delimiter(), normalize(group.stream())),
                ),
                token => token,
            })
            .collect()
    }
    normalize(tokens).to_string()
}

impl InterfaceGroup {
    pub(crate) fn key(&self) -> String {
        identifier_key(self.to_token_stream())
    }
}

impl Parse for InterfaceGroup {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        let mut paths = vec![input.parse::<Path>()?];
        while input.peek(Token![+]) {
            input.parse::<Token![+]>()?;
            paths.push(input.parse()?);
        }
        paths.sort_by_cached_key(|path| identifier_key(path.to_token_stream()));
        paths.dedup_by(|left, right| {
            identifier_key(left.to_token_stream()) == identifier_key(right.to_token_stream())
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

#[derive(Clone)]
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
    fn raw_interface_identifiers_share_one_key_without_rewriting_source_bounds() {
        let raw: InterfaceGroup =
            syn::parse_str("r#module::r#IValue<r#Item = u32> + module::IValue<Item = u32>")
                .unwrap();
        let ordinary: InterfaceGroup = syn::parse_str("module::IValue<Item = u32>").unwrap();
        assert_eq!(raw.0.len(), 1);
        assert_eq!(raw.key(), ordinary.key());
        assert!(raw.to_token_stream().to_string().contains("r#"));
    }

    #[test]
    fn namespaces_are_parsed_for_every_registration_kind() {
        let registrations: Registrations = syn::parse_str(
            "register_value!(value: Value as IValue in test);
             register_type!(Value as IValue in test);
             register_type_with!(Value as IValue in test, || value);
             register_type_with!(Value as IValue in test, try || -> Option<Value> { Some(value) });",
        ).unwrap();
        for registration in registrations.0 {
            assert_eq!(
                registration.namespace.key(&registration.interface),
                "IValue in test"
            );
        }
    }

    #[test]
    fn explicit_default_and_raw_namespace_spellings_are_normalized() {
        let registrations: Registrations = syn::parse_str(
            "register_type!(Value as IValue);
             register_type!(Value as IValue in default);
             register_type!(Value as IValue in named);
             register_type!(Value as IValue in r#named);",
        )
        .unwrap();
        let keys = registrations
            .0
            .iter()
            .map(|registration| registration.namespace.key(&registration.interface))
            .collect::<Vec<_>>();
        assert_eq!(keys[0], keys[1]);
        assert_eq!(keys[2], keys[3]);
        assert_ne!(keys[0], keys[2]);
    }

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
