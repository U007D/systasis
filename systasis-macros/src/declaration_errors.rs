//! Infer and combine declaration initialization errors without caller annotations.

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{parse_quote, visit_mut::VisitMut};

#[derive(Default)]
pub(crate) struct Sources {
    count: usize,
}

impl Sources {
    fn propagate(&mut self, source: &syn::Expr) -> syn::Expr {
        let function = format_ident!("propagate_{}", self.count);
        self.count += 1;
        parse_quote! {
            __systasis_declaration::#function(#source, &mut __systasis_error_marker)
        }
    }

    pub(crate) fn kind(&self) -> TokenStream {
        if self.count == 0 {
            return quote!(__systasis_declaration::infer(&container).kind());
        }
        let sources = (0..self.count).map(|i| format_ident!("source_{i}"));
        quote! {
            __systasis_declaration::Kinds(
                #(__systasis_declaration::#sources(&container).kind(),)*
            )
        }
    }

    pub(crate) fn definitions(&self) -> TokenStream {
        if self.count == 0 {
            return TokenStream::new();
        }
        let errors = (0..self.count)
            .map(|i| format_ident!("E{i}"))
            .collect::<Vec<_>>();
        let kinds = (0..self.count)
            .map(|i| format_ident!("K{i}"))
            .collect::<Vec<_>>();
        let variants = (0..self.count)
            .map(|i| format_ident!("Source{i}"))
            .collect::<Vec<_>>();
        let conversions = errors.iter().enumerate().map(|(i, error)| {
            let propagate = format_ident!("propagate_{i}");
            let source = format_ident!("source_{i}");
            let variant = &variants[i];
            quote! {
                pub fn #propagate<T, #(#errors),*>(
                    result: ::core::result::Result<T, #error>,
                    _: &mut PhantomData<InitializationErrors<#(#errors),*>>,
                ) -> ::core::result::Result<T, InitializationErrors<#(#errors),*>> {
                    result.map_err(InitializationErrors::#variant)
                }
                pub fn #source<T, #(#errors),*>(
                    _: &::core::result::Result<T, InitializationErrors<#(#errors),*>>,
                ) -> Infer<#error> {
                    Infer(PhantomData)
                }
            }
        });
        quote! {
            #[derive(Debug)]
            pub enum InitializationErrors<#(#errors),*> {
                #(#variants(#errors),)*
            }
            impl<#(#errors: ::core::error::Error + 'static),*>
                ::core::fmt::Display for InitializationErrors<#(#errors),*>
            {
                fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                    match self { #(Self::#variants(error) => ::core::fmt::Display::fmt(error, f),)* }
                }
            }
            impl<#(#errors: ::core::error::Error + 'static),*>
                ::core::error::Error for InitializationErrors<#(#errors),*>
            {
                fn source(&self) -> ::core::option::Option<&(dyn ::core::error::Error + 'static)> {
                    ::core::option::Option::Some(match self { #(Self::#variants(error) => error,)* })
                }
            }
            pub struct Kinds<#(#kinds),*>(#(pub #kinds,)*);
            impl<#(#kinds: ErrorKind),*> ErrorKind for Kinds<#(#kinds),*> {
                type Error = InitializationErrors<#(#kinds::Error),*>;
                const NEVER: bool = true #(&& #kinds::NEVER)*;
            }
            #(#conversions)*
        }
    }
}

// Propagation inside a nested function, closure, async block or try block does
// not return from the stored initializer, and must retain ordinary Rust behavior.
impl VisitMut for Sources {
    fn visit_item_mut(&mut self, _: &mut syn::Item) {}
    fn visit_expr_closure_mut(&mut self, _: &mut syn::ExprClosure) {}
    fn visit_expr_async_mut(&mut self, _: &mut syn::ExprAsync) {}
    fn visit_expr_try_block_mut(&mut self, _: &mut syn::ExprTryBlock) {}

    fn visit_expr_try_mut(&mut self, expression: &mut syn::ExprTry) {
        self.visit_expr_mut(&mut expression.expr);
        *expression.expr = self.propagate(&expression.expr);
    }

    fn visit_expr_return_mut(&mut self, expression: &mut syn::ExprReturn) {
        if let Some(source) = &mut expression.expr {
            self.visit_expr_mut(source);
            **source = self.propagate(source);
        }
    }
}
