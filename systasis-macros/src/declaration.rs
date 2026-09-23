//! Declaration form with inferred initialization errors.
use proc_macro2::TokenStream;
use quote::quote;
use syn::{parse_quote, visit_mut::VisitMut};

struct ModuleReferences;

impl VisitMut for ModuleReferences {
    fn visit_path_mut(&mut self, path: &mut syn::Path) {
        syn::visit_mut::visit_path_mut(self, path);
        if path.leading_colon.is_none()
            && let Some(first) = path.segments.first_mut()
            && first.ident == "__systasis_injected"
        {
            first.ident = syn::Ident::new("self", first.ident.span());
        }
        if path.is_ident("SystasisContainer") {
            *path = parse_quote!(Container);
        }
    }
}

pub(crate) fn expand(registrations: TokenStream) -> syn::Result<TokenStream> {
    let function = parse_quote! {
        fn __systasis_build() -> ::core::result::Result<SystasisContainer, __systasis_declaration::NativeError> {
            let mut __systasis_error_marker = ::core::marker::PhantomData;
            let container = ::systasis::systasis_container! {
                #registrations
            }.build();
            let container = __systasis_declaration::tie(container, &mut __systasis_error_marker);
        }
    };
    let mut errors = crate::declaration_errors::Sources::default();
    let mut definitions: syn::File =
        syn::parse2(crate::generate::expand_declaration(function, &mut errors)?)?;
    let mut initializer = None;
    definitions.items.retain(|item| {
        if let syn::Item::Fn(function) = item
            && function.sig.ident == "__systasis_build"
        {
            initializer = Some(function.clone());
            false
        } else {
            true
        }
    });
    let mut initializer = initializer.ok_or_else(|| {
        syn::Error::new_spanned(&definitions, "declaration initializer was not generated")
    })?;
    crate::rebase::Rebase::default().visit_item_fn_mut(&mut initializer);
    ModuleReferences.visit_item_fn_mut(&mut initializer);
    let kind = errors.kind();
    let error_definitions = errors.definitions();
    initializer
        .block
        .stmts
        .push(parse_quote!(let kind = #kind;));
    // The never-selection constant must not depend on the NativeError opaque
    // alias while checking an enclosing function containing a declaration.
    // Infer Kind from the same generated initializer in an uncalled function;
    // only __systasis_build executes registrations at runtime.
    let mut kind_initializer = initializer.clone();
    kind_initializer.sig.ident =
        syn::Ident::new("__systasis_error_kind", initializer.sig.ident.span());
    kind_initializer.attrs = vec![
        parse_quote!(#[define_opaque(__systasis_declaration::Kind)]),
        parse_quote!(#[allow(dead_code)]),
    ];
    kind_initializer.sig.output = parse_quote!(-> __systasis_declaration::Kind);
    kind_initializer
        .block
        .stmts
        .push(syn::Stmt::Expr(parse_quote!(kind), None));
    initializer.attrs = vec![parse_quote!(#[define_opaque(__systasis_declaration::NativeError)])];
    initializer.sig.output =
        parse_quote!(-> ::core::result::Result<Container, __systasis_declaration::NativeError>);
    initializer.block.stmts.push(parse_quote!(
        let _: ::core::marker::PhantomData<__systasis_declaration::NativeError> =
            __systasis_declaration::ErrorKind::marker(&kind);
    ));
    initializer
        .block
        .stmts
        .push(syn::Stmt::Expr(parse_quote!(container), None));
    let helpers: syn::File = syn::parse2(quote! {
        // A function's divergent return type is stable syntax. Projecting it
        // names the real never type without an additional language feature.
        #[doc(hidden)]
        #[allow(dead_code)]
        pub mod __systasis_declaration {
            use ::core::marker::PhantomData;
            pub trait FunctionOutput { type Output; }
            impl<T, F: FnOnce() -> T> FunctionOutput for F { type Output = T; }
            pub type Never = <fn() -> ! as FunctionOutput>::Output;
            pub trait ErrorKind {
                type Error;
                const NEVER: bool;
                fn marker(&self) -> PhantomData<Self::Error> { PhantomData }
            }
            pub struct Impossible;
            impl ErrorKind for Impossible { type Error = Never; const NEVER: bool = true; }
            pub struct Possible<E>(PhantomData<E>);
            impl<E> ErrorKind for Possible<E> { type Error = E; const NEVER: bool = false; }
            pub struct Infer<E>(PhantomData<E>);
            impl Infer<Never> { pub fn kind(self) -> Impossible { Impossible } }
            pub trait ActualError { type Error; fn kind(self) -> Possible<Self::Error>; }
            impl<E> ActualError for Infer<E> {
                type Error = E;
                fn kind(self) -> Possible<E> { Possible(PhantomData) }
            }
            pub fn infer<T, E>(_: &::core::result::Result<T, E>) -> Infer<E> { Infer(PhantomData) }
            pub fn tie<T, E>(result: ::core::result::Result<T, E>, _: &mut PhantomData<E>) -> ::core::result::Result<T, E> { result }

            #error_definitions
            pub type NativeError = impl ::core::error::Error + 'static;
            pub type Kind = impl ErrorKind;
            pub struct Select<const EMPTY: bool>;
            pub trait ErrorSelection<E> { type Error; fn convert(error: E) -> Self::Error; }
            impl<E> ErrorSelection<E> for Select<false> {
                type Error = E;
                fn convert(error: E) -> E { error }
            }
            impl<E> ErrorSelection<E> for Select<true> {
                type Error = Never;
                fn convert(_: E) -> Never {
                    unreachable!("the two generated initializers infer the same source errors; Kind::NEVER is true only when every source error is the never type, so no error variant can exist")
                }
            }
            pub type Choice = Select<{ <Kind as ErrorKind>::NEVER }>;
            pub type Error = <Choice as ErrorSelection<NativeError>>::Error;
        }
        #[allow(unused_imports)]
        use __systasis_declaration::ActualError as _;
        #kind_initializer
        #initializer
        impl Container {
            pub fn build() -> ::core::result::Result<Self, __systasis_declaration::Error> {
                __systasis_build().map_err(
                    <__systasis_declaration::Choice as __systasis_declaration::ErrorSelection<__systasis_declaration::NativeError>>::convert
                )
            }
        }
    })?;
    let module = definitions
        .items
        .iter_mut()
        .find_map(|item| match item {
            syn::Item::Mod(module) if module.ident == "__systasis_injected" => Some(module),
            _ => None,
        })
        .ok_or_else(|| {
            syn::Error::new(
                proc_macro2::Span::call_site(),
                "container module was not generated",
            )
        })?;
    ModuleReferences.visit_item_mod_mut(module);
    module
        .content
        .as_mut()
        .ok_or_else(|| syn::Error::new(module.ident.span(), "container module is empty"))?
        .1
        .extend(helpers.items);
    Ok(quote! {
        #definitions
        // A block-local type alias makes its enclosing function an implicit
        // defining use of NativeError. A re-export exposes the same name
        // without adding that unconstrained defining use.
        pub use __systasis_injected::__systasis_declaration::Error as SystasisContainerError;
    })
}
