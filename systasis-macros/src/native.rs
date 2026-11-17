//! Stable type families for compiler-inferred constructor closures.

use std::collections::BTreeMap;

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{
    ExprPath, GenericArgument, GenericParam, Generics, Ident, Index, PathArguments, Type,
    TypeParamBound, parse_quote, visit_mut::VisitMut,
};

#[derive(Default)]
pub(crate) struct Storage {
    pub(crate) bounds: BTreeMap<usize, TypeParamBound>,
}

pub(crate) fn parameter(index: usize) -> Ident {
    format_ident!("__systasis_Factory{index}")
}

impl Storage {
    pub(crate) fn is_empty(&self) -> bool {
        self.bounds.is_empty()
    }

    pub(crate) fn parameters(&self) -> Vec<Ident> {
        self.bounds.keys().copied().map(parameter).collect()
    }

    pub(crate) fn extend(&self, generics: &mut Generics, indices: &[usize]) {
        for (&index, bound) in &self.bounds {
            if indices.contains(&index) {
                let name = parameter(index);
                generics.params.push(parse_quote!(#name));
                generics
                    .make_where_clause()
                    .predicates
                    .push(parse_quote!(#name: #bound));
            }
        }
    }

    pub(crate) fn project(&self, original: &Generics, owner: Type) -> Projection {
        let (_, arguments, _) = original.split_for_impl();
        let replacements = self.parameters().into_iter().map(|name| {
            let ty = parse_quote!(<#owner as __systasis_injected::__systasis_FactoryTypes #arguments>::#name);
            (name.to_string(), ty)
        }).collect();
        Projection { replacements }
    }

    pub(crate) fn definitions(&self, original: &Generics) -> TokenStream {
        if self.is_empty() {
            return TokenStream::new();
        }
        let names = self.parameters();
        let slots = self
            .bounds
            .keys()
            .map(|&index| {
                let name = parameter(index);
                let wrapper = format_ident!("__systasis_NativeFactory{index}");
                quote!(::systasis::__private::FactorySlot<#wrapper<#name>>)
            })
            .collect::<Vec<_>>();
        let (parameters, arguments, predicates) = original.split_for_impl();
        let mut implementation = original.clone();
        implementation.params.extend(
            names
                .iter()
                .map(|name| -> GenericParam { parse_quote!(#name) }),
        );
        let (impl_parameters, _, impl_predicates) = implementation.split_for_impl();
        let mut family = original.clone();
        self.extend(
            &mut family,
            &self.bounds.keys().copied().collect::<Vec<_>>(),
        );
        let (family_parameters, _, family_predicates) = family.split_for_impl();
        let mut projection = self.project(original, parse_quote!(Self));
        let associated_bounds = self
            .bounds
            .iter()
            .map(|(&index, bound)| {
                let name = parameter(index);
                let mut bound = bound.clone();
                projection.visit_type_param_bound_mut(&mut bound);
                quote!(#name: #bound)
            })
            .collect::<Vec<_>>();
        let original_arguments = original.params.iter().map(|p| match p {
            GenericParam::Lifetime(p) => {
                let name = &p.lifetime;
                quote!(#name)
            }
            GenericParam::Type(p) => {
                let name = &p.ident;
                quote!(#name)
            }
            GenericParam::Const(p) => {
                let name = &p.ident;
                quote!(#name)
            }
        });
        let accessors = self.bounds.keys().map(|&index| {
            let name = parameter(index);
            let wrapper = format_ident!("__systasis_NativeFactory{index}");
            let method = format_ident!("__systasis_factory{index}");
            quote!(#[doc(hidden)] fn #method(&self) -> &::systasis::__private::FactorySlot<#wrapper<Self::#name>>;)
        });
        let implementations = self.bounds.keys().enumerate().map(|(position, &index)| {
            let name = parameter(index);
            let wrapper = format_ident!("__systasis_NativeFactory{index}");
            let method = format_ident!("__systasis_factory{index}");
            let position = Index::from(position);
            quote!(fn #method(&self) -> &::systasis::__private::FactorySlot<#wrapper<Self::#name>> { &self.#position })
        });
        quote! {
            /// Inferred constructor types; the unit implementation names the build entry point.
            #[doc(hidden)]
            pub trait __systasis_FactoryTypes #parameters #predicates {
                #(type #names;)*
            }
            impl #impl_parameters __systasis_FactoryTypes #arguments for (#(#slots,)*) #impl_predicates {
                #(type #names = #names;)*
            }
            impl #parameters __systasis_FactoryTypes #arguments for () #predicates {
                #(type #names = ();)*
            }
            /// The inferred constructors required by this container's resolvers.
            pub trait SystasisFactories #parameters:
                __systasis_FactoryTypes<#(#original_arguments,)* #(#associated_bounds),*> #predicates {
                    #(#accessors)*
                }
            impl #family_parameters SystasisFactories #arguments for (#(#slots,)*) #family_predicates {
                #(#implementations)*
            }
        }
    }

    pub(crate) fn infer(&self) -> Projection {
        Projection {
            replacements: self
                .parameters()
                .into_iter()
                .map(|name| (name.to_string(), parse_quote!(_)))
                .collect(),
        }
    }
}

pub(crate) struct Projection {
    replacements: BTreeMap<String, Type>,
}

impl VisitMut for Projection {
    fn visit_type_mut(&mut self, ty: &mut Type) {
        if let Type::Path(path) = ty
            && let Some(name) = path.path.get_ident()
            && let Some(replacement) = self.replacements.get(&name.to_string())
        {
            *ty = replacement.clone();
            return;
        }
        syn::visit_mut::visit_type_mut(self, ty);
    }
}

/// Authored type arguments precede inferred closure types on constructor helpers.
pub(crate) struct Calls<'a> {
    pub(crate) storage: &'a Storage,
    pub(crate) dependencies: &'a [std::collections::BTreeSet<usize>],
}

impl VisitMut for Calls<'_> {
    fn visit_expr_path_mut(&mut self, path: &mut ExprPath) {
        syn::visit_mut::visit_expr_path_mut(self, path);
        let Some(last) = path.path.segments.last_mut() else {
            return;
        };
        let Some(index) = last
            .ident
            .to_string()
            .strip_prefix("__systasis_construct_")
            .and_then(|index| index.parse::<usize>().ok())
        else {
            return;
        };
        if let PathArguments::AngleBracketed(arguments) = &mut last.arguments {
            let indices = crate::wiring::arguments(index, self.dependencies);
            arguments.args.extend(
                self.storage
                    .bounds
                    .keys()
                    .filter(|index| indices.contains(index))
                    .map(|_| -> GenericArgument { parse_quote!(_) }),
            );
        }
    }
}
