pub(crate) struct Requirements {
    pub(crate) local: bool,
    pub(crate) positive: Vec<syn::Ident>,
}

impl syn::parse::Parse for Requirements {
    fn parse(input: syn::parse::ParseStream<'_>) -> syn::Result<Self> {
        let mut result = Self {
            local: false,
            positive: Vec::new(),
        };
        if input.is_empty() {
            return Ok(result);
        }
        let keyword: syn::Ident = input.parse()?;
        if keyword != "require" {
            return Err(syn::Error::new_spanned(keyword, "expected require(...)"));
        }
        let content;
        syn::parenthesized!(content in input);
        if content.is_empty() {
            return Err(content.error("require expects Send, Sync, or !Sync"));
        }
        while !content.is_empty() {
            let negative = content.parse::<Option<syn::Token![!]>>()?.is_some();
            let name: syn::Ident = content.parse()?;
            match (negative, name.to_string().as_str()) {
                (true, "Sync") if !result.local => result.local = true,
                (false, "Send" | "Sync") if !result.positive.contains(&name) => {
                    result.positive.push(name)
                }
                _ => {
                    return Err(syn::Error::new_spanned(
                        name,
                        "expected distinct Send, Sync, or !Sync requirements",
                    ));
                }
            }
            if content.is_empty() {
                break;
            }
            content.parse::<syn::Token![,]>()?;
        }
        if result.local && result.positive.iter().any(|name| name == "Sync") {
            return Err(syn::Error::new_spanned(
                keyword,
                "Sync and !Sync cannot be required together",
            ));
        }
        if !input.is_empty() {
            return Err(input.error("unexpected container attribute arguments"));
        }
        Ok(result)
    }
}
