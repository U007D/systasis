//! Let rustc select conditional local declarations before capture analysis.

use proc_macro2::TokenStream;
use quote::quote;
use syn::{ItemFn, Meta, Stmt, Token, parse_quote, punctuated::Punctuated, visit_mut::VisitMut};

/// Split only the first conditional local. The compiler discards one branch
/// before re-entry, so independent conditions do not create an eager product
/// of configurations. Each stage copies the function twice; long condition
/// chains still incur expansion depth and cumulative token-processing costs.
pub(crate) fn select(
    function: &ItemFn,
    arguments: &TokenStream,
) -> syn::Result<Option<TokenStream>> {
    let mut present = function.clone();
    let mut absent = function.clone();
    let mut selection = Selection {
        present: true,
        condition: None,
        error: None,
    };
    selection.visit_block_mut(&mut present.block);
    if let Some(error) = selection.error {
        return Err(error);
    }
    let Some(condition) = selection.condition else {
        return Ok(None);
    };
    let mut removal = Selection {
        present: false,
        condition: None,
        error: None,
    };
    removal.visit_block_mut(&mut absent.block);
    if let Some(error) = removal.error {
        return Err(error);
    }
    Ok(Some(quote! {
        #[cfg(#condition)]
        #[::systasis::container(#arguments)]
        #present
        #[cfg(not(#condition))]
        #[::systasis::container(#arguments)]
        #absent
    }))
}

struct Selection {
    present: bool,
    condition: Option<Meta>,
    error: Option<syn::Error>,
}

fn selects_source(meta: &Meta) -> bool {
    if meta.path().is_ident("cfg") {
        return true;
    }
    let Meta::List(list) = meta else { return false };
    list.path.is_ident("cfg_attr")
        && list
            .parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)
            .is_ok_and(|entries| entries.iter().skip(1).any(selects_source))
}

impl VisitMut for Selection {
    fn visit_block_mut(&mut self, block: &mut syn::Block) {
        for index in 0..block.stmts.len() {
            if self.condition.is_some() || self.error.is_some() {
                return;
            }
            if let Stmt::Local(local) = &mut block.stmts[index]
                && let Some(position) = local
                    .attrs
                    .iter()
                    .position(|attr| selects_source(&attr.meta))
            {
                let attribute = local.attrs.remove(position);
                if attribute.path().is_ident("cfg") {
                    match attribute.parse_args::<Meta>() {
                        Ok(condition) => self.condition = Some(condition),
                        Err(error) => self.error = Some(error),
                    }
                    if !self.present {
                        block.stmts.remove(index);
                    }
                } else {
                    match attribute.parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)
                    {
                        Ok(entries) => {
                            let mut entries = entries.into_iter();
                            let Some(condition) = entries.next() else {
                                self.error = Some(syn::Error::new_spanned(
                                    attribute,
                                    "cfg_attr requires a condition",
                                ));
                                return;
                            };
                            self.condition = Some(condition);
                            if self.present {
                                // Rust inserts cfg_attr's attributes at its original
                                // position; appending would reorder caller attributes.
                                local.attrs.splice(
                                    position..position,
                                    entries.map(|meta| parse_quote!(#[#meta])),
                                );
                            }
                        }
                        Err(error) => self.error = Some(error),
                    }
                }
                return;
            }
            self.visit_stmt_mut(&mut block.stmts[index]);
        }
    }

    // Nested items cannot capture this function's locals. Their own compiler
    // configuration/attributes must not be expanded as part of this function.
    fn visit_item_mut(&mut self, _: &mut syn::Item) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use quote::ToTokens;

    #[test]
    fn leaves_non_selection_attributes_and_nested_items_to_rustc() {
        let function = parse_quote!(
            fn example() {
                #[cfg_attr(all())]
                let value: u32 = 1;
                #[cfg_attr(all(), allow(unused_variables))]
                let other: u32 = value;
                fn nested() {
                    #[cfg(any())]
                    let invalid: Missing = missing;
                }
            }
        );
        assert!(select(&function, &TokenStream::new()).unwrap().is_none());
    }

    #[test]
    fn selects_one_local_per_active_expansion() {
        let mut function: ItemFn = parse_quote!(
            fn example() {
                #[cfg(any())]
                let first: u32 = 1;
                #[cfg(any())]
                let second: u32 = 2;
            }
        );
        let mut selection = Selection {
            present: false,
            condition: None,
            error: None,
        };
        selection.visit_block_mut(&mut function.block);
        assert_eq!(function.block.stmts.len(), 1);
        assert!(function.to_token_stream().to_string().contains("cfg"));
    }

    #[test]
    fn cfg_attr_inserts_at_original_position() {
        let mut function: ItemFn = parse_quote!(
            fn example() {
                #[allow(dead_code)]
                #[cfg_attr(all(), allow(unused_variables), cfg(all()), allow(unused_mut))]
                #[allow(unused_assignments)]
                let value: u32 = 1;
            }
        );
        let mut selection = Selection {
            present: true,
            condition: None,
            error: None,
        };
        selection.visit_block_mut(&mut function.block);
        let Stmt::Local(local) = &function.block.stmts[0] else {
            panic!("fixture is a local")
        };
        let attributes = local
            .attrs
            .iter()
            .map(ToTokens::to_token_stream)
            .map(|tokens| tokens.to_string())
            .collect::<Vec<_>>();
        assert_eq!(
            attributes,
            [
                "# [allow (dead_code)]",
                "# [allow (unused_variables)]",
                "# [cfg (all ())]",
                "# [allow (unused_mut)]",
                "# [allow (unused_assignments)]"
            ]
        );
    }
}
