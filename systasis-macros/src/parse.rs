use syn::{
    parse::{Parse, ParseStream},
    *,
};
pub(crate) struct Registration {
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
            if name != "register_value" {
                return Err(Error::new_spanned(
                    name,
                    "this implementation currently supports register_value only",
                ));
            }
            input.parse::<Token![!]>()?;
            let body;
            parenthesized!(body in input);
            entries.push(body.parse()?);
            input.parse::<Token![;]>()?;
        }
        Ok(Self(entries))
    }
}
