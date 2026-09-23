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
            path.segments = path.segments.iter().skip(1).cloned().collect();
        }
        if path.is_ident("SystasisContainer") {
            *path = parse_quote!(__systasis_Container);
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
    initializer.sig.output = parse_quote!(-> ::core::result::Result<__systasis_Container, __systasis_declaration::NativeError>);
    initializer.block.stmts.push(parse_quote!(
        let _: ::core::marker::PhantomData<__systasis_declaration::NativeError> =
            __systasis_declaration::ErrorKind::marker(&kind);
    ));
    initializer
        .block
        .stmts
        .push(syn::Stmt::Expr(parse_quote!(container), None));
    // Keep only generic error machinery in this module. Initializers remain in
    // the invocation scope so they can name the caller's block-local types.
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
        impl __systasis_Container {
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
    // The attribute form places implementation details in a sibling module;
    // declarations must also work inside blocks, whose local types a module
    // cannot import. Keep their items here, using reserved __systasis_* names.
    ModuleReferences.visit_item_mod_mut(module);
    let (_, mut implementation) = module
        .content
        .take()
        .ok_or_else(|| syn::Error::new(module.ident.span(), "container module is empty"))?;
    implementation.retain(|item| !matches!(item, syn::Item::Use(import) if matches!(&import.tree,
        syn::UseTree::Path(path) if path.ident == "super" && matches!(&*path.tree, syn::UseTree::Glob(_)))));
    for item in &mut implementation {
        let visibility = match item {
            syn::Item::Const(item) => Some(&mut item.vis),
            syn::Item::Fn(item) => Some(&mut item.vis),
            syn::Item::Struct(item) => Some(&mut item.vis),
            syn::Item::Type(item) => Some(&mut item.vis),
            _ => None,
        };
        if let Some(visibility) = visibility
            && matches!(&*visibility, syn::Visibility::Restricted(v) if v.path.is_ident("super"))
        {
            *visibility = syn::Visibility::Inherited;
        }
        if let syn::Item::Struct(item) = item {
            for field in &mut item.fields {
                if matches!(&field.vis, syn::Visibility::Restricted(v) if v.path.is_ident("super"))
                {
                    field.vis = syn::Visibility::Inherited;
                }
            }
        }
    }
    implementation.extend(helpers.items);
    definitions.items.retain(
        |item| !matches!(item, syn::Item::Mod(module) if module.ident == "__systasis_injected"),
    );
    for item in &mut definitions.items {
        if let syn::Item::Type(alias) = item {
            ModuleReferences.visit_type_mut(&mut alias.ty);
        }
        if let syn::Item::Mod(module) = item
            && let Some((_, items)) = &mut module.content
        {
            for item in items {
                if let syn::Item::Use(import) = item
                    && let syn::UseTree::Path(parent) = &mut import.tree
                    && let syn::UseTree::Path(generated) = &mut *parent.tree
                    && generated.ident == "__systasis_injected"
                {
                    parent.tree = generated.tree.clone();
                }
            }
        }
    }
    // Re-export instead of adding an alias: a new block-local opaque alias use
    // would make the enclosing function participate in its defining scope.
    Ok(quote! {
        #(
            #[doc(hidden)]
            #[allow(non_snake_case, non_camel_case_types, non_upper_case_globals, unused_imports, dead_code)]
            #implementation
        )*
        #definitions
        /// The inferred initialization error; `source()` exposes the original error.
        pub use __systasis_declaration::Error as SystasisContainerError;
    })
}
