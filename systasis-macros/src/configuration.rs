//! Let rustc select conditional local declarations before capture analysis.

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use std::collections::BTreeSet;
use syn::{
    Attribute, ItemFn, ItemStruct, Meta, Stmt, Token, parse_quote, punctuated::Punctuated,
    visit_mut::VisitMut,
};

/// rustc selects unit fields before invoking the derive. The following erase
/// attribute removes the temporary struct after the derive emits the function.
/// Expansion depth does not depend on the number of conditional statements.
pub(crate) fn select(
    function: &ItemFn,
    arguments: &TokenStream,
) -> syn::Result<Option<TokenStream>> {
    let mut selection = Selection::new(None);
    selection.visit_block_mut(&mut function.block.clone());
    if let Some(error) = selection.error {
        return Err(error);
    }
    if selection.fields.is_empty() {
        return Ok(None);
    }
    let fields = selection.fields;
    let name = format_ident!("__SystasisConfiguration_{}", function.sig.ident);
    Ok(Some(quote! {
        #[derive(::systasis::__private::__SystasisSelectConfiguration)]
        #[::systasis::__private::__systasis_erase_configuration]
        #[__systasis_configuration_source(#function)]
        #[__systasis_configuration_args(#arguments)]
        struct #name { #(#fields)* }
    }))
}

pub(crate) fn resume(input: ItemStruct) -> syn::Result<TokenStream> {
    let attribute = |name: &str| {
        input
            .attrs
            .iter()
            .find(|attr| attr.path().is_ident(name))
            .ok_or_else(|| {
                syn::Error::new_spanned(&input, "missing internal configuration metadata")
            })
    };
    let mut function: ItemFn = attribute("__systasis_configuration_source")?.parse_args()?;
    let requirements: crate::requirements::Requirements =
        attribute("__systasis_configuration_args")?.parse_args()?;
    let active = input
        .fields
        .iter()
        .filter_map(|field| field.ident.as_ref())
        .map(ToString::to_string)
        .collect();
    let mut selection = Selection::new(Some(active));
    selection.visit_block_mut(&mut function.block);
    if let Some(error) = selection.error {
        return Err(error);
    }
    crate::generate::expand(function, requirements.local, &requirements.positive)
}

struct Selection {
    next: usize,
    fields: Vec<TokenStream>,
    active: Option<BTreeSet<String>>,
    gates: Vec<Attribute>,
    error: Option<syn::Error>,
}

impl Selection {
    fn new(active: Option<BTreeSet<String>>) -> Self {
        Self {
            next: 0,
            fields: Vec::new(),
            active,
            gates: Vec::new(),
            error: None,
        }
    }

    fn condition(&mut self, condition: &Attribute) -> bool {
        let name = format_ident!("selected_{}", self.next);
        self.next += 1;
        let gates = &self.gates;
        // Separate ordered attributes preserve cfg's suppression of subsequent
        // invalid predicates. Combining them into all(...) would not do that.
        self.fields.push(quote!(#(#gates)* #condition #name: (),));
        self.active
            .as_ref()
            .is_none_or(|active| active.contains(&name.to_string()))
    }

    fn attributes(&mut self, attributes: Vec<Attribute>) -> syn::Result<(bool, Vec<Attribute>)> {
        let before = self.gates.len();
        let mut present = true;
        let mut output = Vec::new();
        for attribute in attributes {
            let gate = selection_meta(&attribute.meta)?;
            if attribute.path().is_ident("cfg") {
                // Keep predicate tokens opaque: rustc diagnoses malformed cfg
                // only when its preceding ancestry has not discarded the field.
                present &= self.condition(&attribute);
            } else if attribute.path().is_ident("cfg_attr") && gate.is_some() {
                let (condition, children) = cfg_attr_arguments(&attribute.meta)?;
                let enabled = self.condition(&parse_quote!(#[cfg(#condition)]));
                self.gates.push(parse_quote!(#[cfg(#condition)]));
                let (child_present, expanded) = self.attributes(
                    children
                        .into_iter()
                        .map(|meta| parse_quote!(#[#meta]))
                        .collect(),
                )?;
                self.gates.pop();
                if enabled {
                    present &= child_present;
                    output.extend(expanded);
                }
            } else {
                output.push(attribute);
            }
            if let Some(gate) = gate {
                self.gates.push(parse_quote!(#[#gate]));
            }
        }
        self.gates.truncate(before);
        Ok((present, output))
    }
}

fn cfg_attr_arguments(meta: &Meta) -> syn::Result<(Meta, Vec<Meta>)> {
    let Meta::List(list) = meta else {
        return Err(syn::Error::new_spanned(meta, "expected cfg_attr arguments"));
    };
    let mut entries = list
        .parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)?
        .into_iter();
    let condition = entries
        .next()
        .ok_or_else(|| syn::Error::new_spanned(meta, "cfg_attr requires a condition"))?;
    Ok((condition, entries.collect()))
}

/// Copy only configuration predicates onto helper fields, never caller macros
/// or unrelated attributes that belong on the original statement.
fn selection_meta(meta: &Meta) -> syn::Result<Option<Meta>> {
    if meta.path().is_ident("cfg") {
        return Ok(Some(meta.clone()));
    }
    if !selects_source(meta) {
        return Ok(None);
    }
    let (condition, children) = cfg_attr_arguments(meta)?;
    let selected = children
        .iter()
        .map(selection_meta)
        .collect::<syn::Result<Vec<_>>>()?
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    Ok(Some(parse_quote!(cfg_attr(#condition, #(#selected),*))))
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
        block.stmts = std::mem::take(&mut block.stmts)
            .into_iter()
            .filter_map(|mut statement| {
                if self.error.is_some() {
                    return Some(statement);
                }
                let before = self.gates.len();
                let attributes = match &mut statement {
                    Stmt::Local(local) => Some(&mut local.attrs),
                    Stmt::Expr(expression, _) => expression_attributes(expression),
                    Stmt::Macro(statement) => Some(&mut statement.attrs),
                    Stmt::Item(_) => None,
                };
                let mut present = true;
                if let Some(attributes) = attributes {
                    let original = attributes
                        .iter()
                        .map(|attr| selection_meta(&attr.meta))
                        .collect::<syn::Result<Vec<_>>>();
                    match original.and_then(|gates| {
                        self.attributes(std::mem::take(attributes))
                            .map(|result| (gates, result))
                    }) {
                        Ok((gates, (enabled, expanded))) => {
                            present = enabled;
                            *attributes = expanded;
                            self.gates.extend(
                                gates
                                    .into_iter()
                                    .flatten()
                                    .map(|meta| parse_quote!(#[#meta])),
                            );
                        }
                        Err(error) => {
                            self.error = Some(error);
                            return Some(statement);
                        }
                    }
                }
                // Both passes traverse discarded descendants to keep selector IDs
                // aligned; their fields inherit the ancestor's disabling attributes.
                self.visit_stmt_mut(&mut statement);
                self.gates.truncate(before);
                present.then_some(statement)
            })
            .collect();
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
    fn selects_all_locals_in_one_pass() {
        let mut function: ItemFn = parse_quote!(
            fn example() {
                #[cfg(any())]
                let first: u32 = 1;
                #[cfg(any())]
                let second: u32 = 2;
            }
        );
        let mut selection = Selection::new(Some(BTreeSet::new()));
        selection.visit_block_mut(&mut function.block);
        assert_eq!(function.block.stmts.len(), 0);
        assert_eq!(selection.fields.len(), 2);
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
        let mut selection = Selection::new(None);
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
                "# [allow (unused_mut)]",
                "# [allow (unused_assignments)]"
            ]
        );
    }
}
