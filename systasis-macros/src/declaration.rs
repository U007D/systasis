//! Declaration form with inferred initialization errors.
use proc_macro2::TokenStream;
use quote::quote;
use syn::{parse_quote, visit_mut::VisitMut};

struct ModuleReferences;

impl VisitMut for ModuleReferences {
    fn visit_type_path_mut(&mut self, path: &mut syn::TypePath) {
        let segments = path.path.segments.len();
        syn::visit_mut::visit_type_path_mut(self, path);
        if let Some(qualified) = &mut path.qself {
            qualified.position = qualified
                .position
                .saturating_sub(segments.saturating_sub(path.path.segments.len()));
        }
    }

    fn visit_path_mut(&mut self, path: &mut syn::Path) {
        syn::visit_mut::visit_path_mut(self, path);
        if path.leading_colon.is_none()
            && let Some(first) = path.segments.first_mut()
            && first.ident == "__systasis_injected"
        {
            path.segments = path.segments.iter().skip(1).cloned().collect();
        }
        if path.leading_colon.is_none()
            && path
                .segments
                .first()
                .is_some_and(|segment| segment.ident == "self")
            && path
                .segments
                .iter()
                .nth(1)
                .is_some_and(|segment| segment.ident == "__systasis_injected")
        {
            path.segments = path.segments.iter().skip(2).cloned().collect();
        }
        if path.is_ident("SystasisContainer") {
            *path = parse_quote!(__systasis_Container);
        }
    }
}

pub(crate) fn expand(registrations: TokenStream) -> syn::Result<TokenStream> {
    let function = parse_quote! {
        fn __systasis_build() {
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
    let has_factories = definitions.items.iter().any(|item| matches!(item,
        syn::Item::Type(alias) if alias.ident == "SystasisContainer"
            && alias.generics.type_params().any(|parameter| parameter.ident == "__systasis_Factories")));
    let mut raw_generics = definitions
        .items
        .iter()
        .find_map(|item| {
            let syn::Item::Mod(module) = item else {
                return None;
            };
            module
                .content
                .as_ref()?
                .1
                .iter()
                .find_map(|item| match item {
                    syn::Item::Type(alias) if alias.ident == "__systasis_RawContainer" => {
                        Some(alias.generics.clone())
                    }
                    _ => None,
                })
        })
        .ok_or_else(|| {
            syn::Error::new_spanned(&definitions, "raw container alias was not generated")
        })?;
    for parameter in raw_generics.type_params_mut() {
        if parameter.ident == "__systasis_Error" {
            parameter.ident = syn::Ident::new("E", parameter.ident.span());
        }
    }
    let native_names = raw_generics
        .type_params()
        .filter(|parameter| parameter.ident != "E")
        .map(|parameter| &parameter.ident)
        .collect::<Vec<_>>();
    let (raw_parameters, raw_arguments, _) = raw_generics.split_for_impl();
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
    // Inspect the initializer's inferred error types without executing it.
    // A function-item argument exposes the marker constant on stable Rust.
    let mut kind_initializer = initializer.clone();
    kind_initializer.sig.ident =
        syn::Ident::new("__systasis_error_kind", initializer.sig.ident.span());
    kind_initializer.attrs = vec![parse_quote!(#[allow(dead_code)])];
    kind_initializer.sig.output = parse_quote!(-> impl __systasis_declaration::ErrorKind);
    kind_initializer
        .block
        .stmts
        .push(syn::Stmt::Expr(parse_quote!(kind), None));
    let mut parameters = Vec::new();
    let mut arguments = Vec::new();
    let mut opaque = Vec::new();
    if has_factories {
        parameters.push(quote!(F: __systasis_FactoryTypes));
        arguments.push(quote!(F));
        opaque.push(quote!(impl SystasisFactories));
    }
    if errors.has_sources() {
        parameters.push(quote!(E));
        arguments.push(quote!(E));
        opaque.push(quote!(impl ::core::error::Error + 'static));
    }
    let parameters = (!parameters.is_empty()).then(|| quote!(<#(#parameters),*>));
    let arguments = (!arguments.is_empty()).then(|| quote!(<#(#arguments),*>));
    let opaque = (!opaque.is_empty()).then(|| quote!(<#(#opaque),*>));
    let mut finished_arguments = Vec::new();
    if has_factories {
        let slots = native_names.iter().map(|name| {
            let wrapper = quote::format_ident!(
                "{}",
                name.to_string()
                    .replace("__systasis_Factory", "__systasis_NativeFactory")
            );
            quote!(::systasis::__private::FactorySlot<#wrapper<#name>>)
        });
        finished_arguments.push(quote!((#(#slots,)*)));
    }
    if errors.has_sources() {
        finished_arguments.push(quote!(E));
    }
    let finished_arguments =
        (!finished_arguments.is_empty()).then(|| quote!(<#(#finished_arguments),*>));
    initializer.attrs.clear();
    initializer.sig.output = parse_quote!(-> BuildResult #opaque);
    initializer
        .block
        .stmts
        .push(parse_quote!(let result = __systasis_finish(container);));
    initializer
        .block
        .stmts
        .push(syn::Stmt::Expr(parse_quote!(result), None));
    let (error, selected_error, error_definition, conversion) = if errors.has_sources() {
        (
            quote!(E),
            quote!(SystasisContainerError<E>),
            quote! {
                /// The inferred initialization error; `source()` exposes the original error.
                pub type SystasisContainerError<E> = <__systasis_declaration::Select<{ __systasis_declaration::is_never(&__systasis_error_kind) }> as __systasis_declaration::ErrorSelection<E>>::Error;
            },
            quote!(result.map_err(
                <__systasis_declaration::Select<
                    { __systasis_declaration::is_never(&__systasis_error_kind) },
                > as __systasis_declaration::ErrorSelection<E>>::convert
            )),
        )
    } else {
        (
            quote!(::systasis::__private::Never),
            quote!(SystasisContainerError),
            quote! {
                /// The never type for a declaration with no initialization errors.
                pub type SystasisContainerError = ::systasis::__private::Never;
            },
            quote!(result),
        )
    };
    let result_definitions = quote! {
        #error_definition
        /// A build result sharing its inferred type parameters with the container.
        #[allow(type_alias_bounds)]
        pub type BuildResult #parameters = ::core::result::Result<__systasis_Container #arguments, #selected_error>;
        fn __systasis_finish #raw_parameters(result: ::core::result::Result<__systasis_RawContainer #raw_arguments, #error>) -> BuildResult #finished_arguments {
            #conversion
        }
        impl __systasis_Container {
            pub fn build() -> BuildResult #opaque { __systasis_build() }
        }
    };
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
            pub fn container_marker<M, G, E>(_: &PhantomData<E>) -> PhantomData<(M, fn() -> (G, E))> { PhantomData }
            pub const fn is_never<F, K>(_: &F) -> bool where F: FnOnce() -> K, K: ErrorKind { K::NEVER }

            #error_definitions
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
        }
        #[allow(unused_imports)]
        use __systasis_declaration::ActualError as _;
        #kind_initializer
        #initializer
        #result_definitions
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
        |item| !matches!(item, syn::Item::Mod(module) if module.ident == "__systasis_injected")
            && !matches!(item, syn::Item::Use(import) if matches!(&import.tree,
                syn::UseTree::Path(root) if root.ident == "self"
                    && matches!(&*root.tree, syn::UseTree::Path(path) if path.ident == "__systasis_injected"
                        && matches!(&*path.tree, syn::UseTree::Name(name) if name.ident == "SystasisFactories")))),
    );
    for item in &mut definitions.items {
        if let syn::Item::Type(alias) = item {
            ModuleReferences.visit_item_type_mut(alias);
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
    })
}
