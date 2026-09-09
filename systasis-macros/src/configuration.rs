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
    let mut absent = function.clone();
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

fn expression_attributes(expression: &mut syn::Expr) -> Option<&mut Vec<syn::Attribute>> {
    use syn::Expr;
    Some(match expression {
        Expr::Array(value) => &mut value.attrs,
        Expr::Assign(value) => &mut value.attrs,
        Expr::Async(value) => &mut value.attrs,
        Expr::Await(value) => &mut value.attrs,
        Expr::Binary(value) => &mut value.attrs,
        Expr::Block(value) => &mut value.attrs,
        Expr::Break(value) => &mut value.attrs,
        Expr::Call(value) => &mut value.attrs,
        Expr::Cast(value) => &mut value.attrs,
        Expr::Closure(value) => &mut value.attrs,
        Expr::Const(value) => &mut value.attrs,
        Expr::Continue(value) => &mut value.attrs,
        Expr::Field(value) => &mut value.attrs,
        Expr::ForLoop(value) => &mut value.attrs,
        Expr::Group(value) => &mut value.attrs,
        Expr::If(value) => &mut value.attrs,
        Expr::Index(value) => &mut value.attrs,
        Expr::Infer(value) => &mut value.attrs,
        Expr::Let(value) => &mut value.attrs,
        Expr::Lit(value) => &mut value.attrs,
        Expr::Loop(value) => &mut value.attrs,
        Expr::Macro(value) => &mut value.attrs,
        Expr::Match(value) => &mut value.attrs,
        Expr::MethodCall(value) => &mut value.attrs,
        Expr::Paren(value) => &mut value.attrs,
        Expr::Path(value) => &mut value.attrs,
        Expr::Range(value) => &mut value.attrs,
        Expr::RawAddr(value) => &mut value.attrs,
        Expr::Reference(value) => &mut value.attrs,
        Expr::Repeat(value) => &mut value.attrs,
        Expr::Return(value) => &mut value.attrs,
        Expr::Struct(value) => &mut value.attrs,
        Expr::Try(value) => &mut value.attrs,
        Expr::TryBlock(value) => &mut value.attrs,
        Expr::Tuple(value) => &mut value.attrs,
        Expr::Unary(value) => &mut value.attrs,
        Expr::Unsafe(value) => &mut value.attrs,
        Expr::While(value) => &mut value.attrs,
        Expr::Yield(value) => &mut value.attrs,
        _ => return None,
    })
}

impl VisitMut for Selection {
    fn visit_block_mut(&mut self, block: &mut syn::Block) {
        for index in 0..block.stmts.len() {
            if self.condition.is_some() || self.error.is_some() {
                return;
            }
            let attributes = match &mut block.stmts[index] {
                Stmt::Local(local) => Some(&mut local.attrs),
                Stmt::Expr(expression, _) => expression_attributes(expression),
                Stmt::Macro(statement) => Some(&mut statement.attrs),
                Stmt::Item(_) => None,
            };
            if let Some(attributes) = attributes
                && let Some(position) = attributes
                    .iter()
                    .position(|attr| selects_source(&attr.meta))
            {
                let attribute = attributes.remove(position);
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
                                attributes.splice(
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

    fn visit_expr_mut(&mut self, expression: &mut syn::Expr) {
        // Conditional expressions outside statement position remain rustc's
        // responsibility. Never lift predicates from beneath an unselected
        // ancestor: inactive descendants need not contain valid cfg predicates.
        if expression_attributes(expression)
            .is_some_and(|attrs| attrs.iter().any(|attr| selects_source(&attr.meta)))
        {
            return;
        }
        syn::visit_mut::visit_expr_mut(self, expression);
    }

    fn visit_arm_mut(&mut self, arm: &mut syn::Arm) {
        if arm.attrs.iter().any(|attr| selects_source(&attr.meta)) {
            return;
        }
        syn::visit_mut::visit_arm_mut(self, arm);
    }

    fn visit_field_value_mut(&mut self, field: &mut syn::FieldValue) {
        if field.attrs.iter().any(|attr| selects_source(&attr.meta)) {
            return;
        }
        syn::visit_mut::visit_field_value_mut(self, field);
    }
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
