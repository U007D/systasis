use syn::{
    parse::{Parse, ParseStream},
    *,
};
pub(crate) struct Registration {
    pub(crate) fresh: bool,
    pub(crate) constructor: Option<ExprClosure>,
    pub(crate) fallible: bool,
    pub(crate) dynamic: bool,
    pub(crate) value: Expr,
    pub(crate) ty: Type,
    pub(crate) interface: Path,
}
impl Parse for Registration {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        let value = input.parse()?;
        input.parse::<Token![:]>()?;
        let ty = input.parse()?;
        input.parse::<Token![as]>()?;
        let dynamic = input.parse::<Option<Token![dyn]>>()?.is_some();
        Ok(Self {
            fresh: false,
            constructor: None,
            fallible: false,
            dynamic,
            value,
            ty,
            interface: input.parse()?,
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
