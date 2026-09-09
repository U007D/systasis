use syn::{
    parse::{Parse, ParseStream},
    *,
};
pub(crate) struct Registration {
    pub(crate) fresh: bool,
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
        Ok(Self {
            fresh: false,
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
            if name != "register_value" && name != "register_type" {
                return Err(Error::new_spanned(
                    name,
                    "expected register_value! or register_type!",
                ));
            }
            input.parse::<Token![!]>()?;
            let body;
            parenthesized!(body in input);
            entries.push(if name == "register_type" {
                let ty = body.parse()?;
                body.parse::<Token![as]>()?;
                Registration {
                    fresh: true,
                    value: parse_quote!(()),
                    ty,
                    interface: body.parse()?,
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
