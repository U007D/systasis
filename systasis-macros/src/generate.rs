use crate::analysis::{Queries, TypeLookup};
use crate::parse::Registrations;
use proc_macro2::Span;
use quote::{ToTokens, format_ident, quote};
use std::collections::{BTreeMap, BTreeSet};
use syn::{ext::IdentExt, visit_mut::VisitMut, *};
struct Lifetimes(Vec<Lifetime>);
struct CallLifetime(Lifetime);
// Follow only the receiver/postfix chain, not unrelated arguments or blocks.
// The owner's statements must remain in the enclosing scope; only the build
// expression is replaced with its published Result.
fn build_expression(expression: &mut Expr) -> Option<&mut Expr> {
    let is_build = matches!(expression, Expr::MethodCall(call)
        if call.method == "build" && matches!(&*call.receiver, Expr::Macro(registry)
            if registry.mac.path.segments.last().is_some_and(|segment| segment.ident == "systasis_container")));
    if is_build {
        return Some(expression);
    }
    match expression {
        Expr::Try(value) => build_expression(&mut value.expr),
        Expr::Paren(value) => build_expression(&mut value.expr),
        Expr::Group(value) => build_expression(&mut value.expr),
        Expr::MethodCall(value) => build_expression(&mut value.receiver),
        _ => None,
    }
}
impl VisitMut for CallLifetime {
    fn visit_lifetime_mut(&mut self, lifetime: &mut Lifetime) {
        if lifetime.ident == "_" {
            *lifetime = self.0.clone();
        }
    }
}
impl VisitMut for Lifetimes {
    fn visit_lifetime_mut(&mut self, lifetime: &mut Lifetime) {
        if lifetime.ident == "_" {
            *lifetime = Lifetime::new(
                &format!("'__systasis_type_{}", self.0.len()),
                Span::mixed_site(),
            );
            self.0.push(lifetime.clone());
        }
    }
}

pub(crate) fn expand(
    mut function: ItemFn,
    local_policy: bool,
    requirements: &[Ident],
) -> Result<proc_macro2::TokenStream> {
    let original_generics = &function.sig.generics;
    let type_arguments = original_generics
        .params
        .iter()
        .filter_map(|parameter| match parameter {
            GenericParam::Type(parameter) => {
                let name = &parameter.ident;
                Some(quote!(#name))
            }
            GenericParam::Const(parameter) => {
                let name = &parameter.ident;
                Some(quote!(#name))
            }
            GenericParam::Lifetime(_) => None,
        })
        .collect::<Vec<_>>();
    let turbofish = (!type_arguments.is_empty()).then(|| quote!(::<#(#type_arguments),*>));
    let systasis_error_ident = Ident::new("__systasis_error", Span::mixed_site());
    let systasis_value_ident = Ident::new("__systasis_value", Span::mixed_site());
    let systasis_result_ident = Ident::new("__systasis_result", Span::mixed_site());
    let systasis_owner_ident = Ident::new("__systasis_owner", Span::mixed_site());
    let systasis_published_ident = Ident::new("__systasis_published", Span::mixed_site());
    let slot_type = if local_policy {
        quote!(::systasis::__private::LocalTakeSlot)
    } else {
        quote!(::systasis::__private::TakeSlot)
    };
    let marker = if local_policy {
        quote!(::core::cell::Cell<()>)
    } else {
        quote!(())
    };
    let read_type = if local_policy {
        quote!(::core::cell::Ref)
    } else {
        quote!(::systasis::__private::Ref)
    };
    let write_type = if local_policy {
        quote!(::core::cell::RefMut)
    } else {
        quote!(::systasis::__private::RefMut)
    };
    let mut emitted = None;
    let mut statements = Vec::new();
    let mut bindings = crate::captures::Bindings::from_function(&function);
    for (statement_index, statement) in function.block.stmts.iter().enumerate() {
        if statement_index > 0 {
            bindings.observe_statement(&function.block.stmts[statement_index - 1]);
        }
        let Stmt::Local(local) = statement else {
            statements.push(statement.clone());
            continue;
        };
        let Some(initializer) = &local.init else {
            statements.push(statement.clone());
            continue;
        };
        let mut published_expression = *initializer.expr.clone();
        let Some(expression) = build_expression(&mut published_expression) else {
            statements.push(statement.clone());
            continue;
        };
        let Expr::MethodCall(build) = expression.clone() else {
            unreachable!("build_expression only selects a build method call");
        };
        *expression = parse_quote!(#systasis_published_ident);
        let Expr::Macro(registry) = &*build.receiver else {
            statements.push(statement.clone());
            continue;
        };
        if build.method != "build"
            || !registry
                .mac
                .path
                .segments
                .last()
                .is_some_and(|segment| segment.ident == "systasis_container")
        {
            statements.push(statement.clone());
            continue;
        }
        if emitted.is_some() {
            return Err(Error::new_spanned(
                &build,
                "only one container definition per function is currently supported",
            ));
        }
        if !build.args.is_empty()
            || build
                .turbofish
                .as_ref()
                .is_some_and(|args| args.args.len() != 1)
        {
            return Err(Error::new_spanned(
                &build,
                "build accepts no arguments and at most one error type",
            ));
        }
        let Registrations(mut registrations) = syn::parse2(registry.mac.tokens.clone())?;
        // Discard superseded declarations before examining their expressions,
        // types, or dependencies. The last declaration of an interface wins.
        let winners = registrations
            .iter()
            .enumerate()
            .map(|(i, registration)| (registration.interface.to_token_stream().to_string(), i))
            .collect::<BTreeMap<_, _>>();
        registrations = registrations
            .into_iter()
            .enumerate()
            .filter_map(|(i, registration)| {
                (winners[&registration.interface.to_token_stream().to_string()] == i)
                    .then_some(registration)
            })
            .collect();
        let error_ty = build
            .turbofish
            .as_ref()
            .and_then(|a| a.args.first())
            .cloned()
            .unwrap_or(parse_quote!(_));
        let indices = registrations
            .iter()
            .enumerate()
            .map(|(i, r)| (r.interface.to_token_stream().to_string(), i))
            .collect::<BTreeMap<_, _>>();
        let mut dependencies = Vec::new();
        let mut constructor_borrows = BTreeSet::new();
        let original_types = registrations
            .iter()
            .map(|r| r.ty.clone())
            .collect::<Vec<_>>();
        let dynamic = registrations.iter().map(|r| r.dynamic).collect::<Vec<_>>();
        for registration in &mut registrations {
            let mut lookup = TypeLookup {
                indices: &indices,
                types: &original_types,
                dynamic: &dynamic,
                active: Vec::new(),
                dependencies: BTreeSet::new(),
                error: None,
            };
            lookup.visit_type_mut(&mut registration.ty);
            lookup.visit_expr_mut(&mut registration.value);
            if let Some(closure) = &mut registration.constructor {
                lookup.visit_expr_closure_mut(closure);
            }
            if let Some(error) = lookup.error {
                return Err(error);
            }
            let mut queries = Queries {
                indices: &indices,
                dependencies: lookup.dependencies,
                borrowed: BTreeSet::new(),
                error: None,
                replacements: None,
            };
            queries.visit_expr_mut(&mut registration.value);
            if let Some(closure) = &mut registration.constructor {
                queries.visit_expr_closure_mut(closure);
                constructor_borrows.extend(&queries.borrowed);
            }
            if let Some(error) = queries.error {
                return Err(error);
            }
            dependencies.push(queries.dependencies);
        }
        let initializer_types = registrations
            .iter()
            .map(|r| r.ty.clone())
            .collect::<Vec<_>>();
        let order = crate::graph::schedule(&dependencies).map_err(|cycle| {
            let names = cycle
                .iter()
                .map(|&i| registrations[i].interface.to_token_stream().to_string())
                .collect::<Vec<_>>();
            Error::new_spanned(
                registry,
                format!("registration dependency cycle: {}", names.join(" -> ")),
            )
        })?;
        let transitive = crate::wiring::transitive(&dependencies, &order);
        let mut build_queries = crate::wiring::replacements(
            &registrations,
            &transitive,
            true,
            local_policy,
            &turbofish,
        );
        let mut runtime_queries = crate::wiring::replacements(
            &registrations,
            &transitive,
            false,
            local_policy,
            &turbofish,
        );
        for &index in &constructor_borrows {
            build_queries.remove(&(index, "try_resolve".into()));
            runtime_queries.remove(&(index, "try_resolve".into()));
        }
        let capture_root = Ident::new("__systasis_captures", Span::mixed_site());
        let mut factories = BTreeMap::new();
        for (index, registration) in registrations.iter_mut().enumerate() {
            let mut queries = Queries {
                indices: &indices,
                dependencies: BTreeSet::new(),
                borrowed: BTreeSet::new(),
                error: None,
                replacements: Some(&build_queries),
            };
            queries.visit_expr_mut(&mut registration.value);
            if let Some(closure) = &mut registration.constructor {
                queries.replacements = Some(&runtime_queries);
                queries.visit_expr_closure_mut(closure);
                factories.insert(
                    index,
                    crate::captures::prepare(closure, &bindings, &capture_root)?,
                );
            }
            if let Some(error) = queries.error {
                return Err(error);
            }
        }
        let slots = (0..registrations.len())
            .map(|i| format_ident!("__systasis_slot_{i}", span = Span::mixed_site()))
            .collect::<Vec<_>>();
        let fields = (0..registrations.len())
            .map(|i| format_ident!("value_{i}"))
            .collect::<Vec<_>>();

        let flags = (0..registrations.len())
            .map(|i| format_ident!("COPY_{i}"))
            .collect::<Vec<_>>();
        let mut constants = Vec::new();
        let mut policies = Vec::new();
        let mut policy_checks = Vec::new();
        let mut lifetimes = Lifetimes(Vec::new());
        for (i, registration) in registrations.iter_mut().enumerate() {
            let original = &registration.ty;
            let flag = &flags[i];
            if crate::generic_policy::depends_on_generics(original, original_generics) {
                let copy =
                    crate::generic_policy::has_explicit_copy_bound(original, original_generics);
                policies.push(quote!(#copy));
                if !copy && !registration.fresh {
                    policy_checks.push(quote!({
                        use ::systasis::__private::DetectCopy as _;
                        ::systasis::__private::verify_generic_fallback(
                            (&&::systasis::__private::Pick::<#original>::NEW).evidence()
                        );
                    }));
                }
            } else {
                policies.push(quote!({__systasis_injected::#flag}));
                constants.push(quote!(pub(super) const #flag: bool = ::systasis::__private::Pick::<#original>::IS_COPY;));
            }
            lifetimes.visit_type_mut(&mut registration.ty);
        }
        let mut capture_types = BTreeMap::new();
        for (&index, factory) in &factories {
            let mut types = factory
                .captures
                .iter()
                .map(|(_, ty)| ty.clone())
                .collect::<Vec<_>>();
            for ty in &mut types {
                lifetimes.visit_type_mut(ty);
            }
            capture_types.insert(index, types);
        }
        let mut selected = registrations.iter().enumerate().map(|(i,r)| {
            let ty = &r.ty;
            let policy = &policies[i];
            if let Some(types) = capture_types.get(&i) { quote!(::systasis::__private::FactorySlot<(#(#types,)*)>) }
            else if r.fresh { quote!(::systasis::__private::FreshSlot<#ty>) }
            else { quote!(<::systasis::__private::Policy<#policy, #local_policy> as ::systasis::__private::Select<#ty>>::Slot) }
        }).collect::<Vec<_>>();
        let mut generics = function.sig.generics.clone();
        for lifetime in &lifetimes.0 {
            generics.params.insert(0, parse_quote!(#lifetime));
        }
        let mut const_markers = Vec::new();
        let marker_types = generics
            .params
            .iter()
            .map(|parameter| match parameter {
                GenericParam::Type(parameter) => {
                    let name = &parameter.ident;
                    quote!(*const #name)
                }
                GenericParam::Lifetime(parameter) => {
                    let name = &parameter.lifetime;
                    quote!(&#name ())
                }
                GenericParam::Const(parameter) => {
                    let name = &parameter.ident;
                    let ty = &parameter.ty;
                    let marker_name = format_ident!("__Const_{name}");
                    const_markers.push(quote!(pub struct #marker_name<const __VALUE: #ty>;));
                    quote!(__systasis_injected::#marker_name<#name>)
                }
            })
            .collect::<Vec<_>>();
        let generic_marker = quote!(fn() -> (#(#marker_types,)*));
        selected.push(generic_marker.clone());
        let parameters = (0..=registrations.len())
            .map(|i| format_ident!("__Slot{i}"))
            .collect::<Vec<_>>();
        let mut field_types = Vec::new();
        let mut values = Vec::new();
        let mut implementations = Vec::new();
        let mut constructor_functions = Vec::new();
        let mut validation_calls = Vec::new();
        let mut method_names = BTreeMap::<String, crate::parse::InterfaceGroup>::new();
        for (i, registration) in registrations.iter().enumerate() {
            let check = format_ident!("check_{i}");
            let ty = &registration.ty;
            let interface = &registration.interface;
            let mut checked = generics.clone();
            let inherited = checked
                .type_params_mut()
                .filter_map(|parameter| {
                    if parameter.bounds.is_empty() {
                        return None;
                    }
                    let name = &parameter.ident;
                    let bounds = core::mem::take(&mut parameter.bounds);
                    Some(parse_quote!(#name: #bounds))
                })
                .collect::<Vec<WherePredicate>>();
            checked.make_where_clause().predicates.extend(inherited);
            checked
                .make_where_clause()
                .predicates
                .push(parse_quote!(#ty: #interface));
            if registration.fresh && registration.constructor.is_none() {
                checked
                    .make_where_clause()
                    .predicates
                    .push(parse_quote!(#ty: ::core::default::Default));
            }
            let (checked_parameters, _, checked_where) = checked.split_for_impl();
            constructor_functions.push(quote!(
                pub(super) fn #check #checked_parameters (_: ::core::marker::PhantomData<#ty>) #checked_where {}
            ));
            let original = &initializer_types[i];
            validation_calls.push(quote!(__systasis_injected::#check #turbofish (::core::marker::PhantomData::<#original>);));
            let field = &fields[i];
            let slot = &slots[i];
            let storage = &selected[i];
            field_types.push(storage.clone());
            values.push(quote!(#field: #slot.unwrap_or_else(|| ::core::unreachable!("successful build initialized every slot"))));
            let mut names = registration
                .interface
                .0
                .iter()
                .map(|path| path.segments.last().unwrap().ident.unraw().to_string())
                .collect::<Vec<_>>();
            names.sort();
            let snake = names
                .iter()
                .map(|name| {
                    name.chars()
                        .enumerate()
                        .flat_map(|(i, c)| {
                            if c.is_uppercase() && i > 0 {
                                vec!['_', c.to_ascii_lowercase()]
                            } else {
                                vec![c.to_ascii_lowercase()]
                            }
                        })
                        .collect::<String>()
                })
                .collect::<Vec<_>>()
                .join("_");
            if let Some(previous) =
                method_names.insert(snake.clone(), registration.interface.clone())
            {
                let mut error = Error::new_spanned(
                    &registration.interface,
                    "interfaces generate the same resolver name",
                );
                error.combine(Error::new_spanned(previous, "first conflicting interface"));
                return Err(error);
            }
            let read = format_ident!("try_resolve_{snake}_ref");
            let write = format_ident!("try_resolve_{snake}_ref_mut");
            let take = format_ident!("try_resolve_{snake}");
            let copy = format_ident!("resolve_{snake}");
            let copy_ref = format_ident!("resolve_{snake}_ref");
            let clone = format_ident!("resolve_{snake}_clone");
            let try_clone = format_ident!("try_resolve_{snake}_clone");
            if let Some(factory) = factories.get(&i) {
                let helper = format_ident!("construct_{i}");
                let output = match &factory.closure.output {
                    ReturnType::Default => initializer_types[i].clone(),
                    ReturnType::Type(_, ty) => (**ty).clone(),
                };
                let closure = &factory.closure;
                let call_lifetime = Lifetime::new("'__systasis_call", Span::mixed_site());
                let mut helper_output = output.clone();
                CallLifetime(call_lifetime.clone()).visit_type_mut(&mut helper_output);
                let mut helper_generics = generics.clone();
                helper_generics
                    .params
                    .insert(0, parse_quote!(#call_lifetime));
                let arguments = crate::wiring::arguments(i, &transitive);
                let helper_parameters = arguments.iter().map(|&index| {
                    let parameter = &slots[index];
                    let ty = &selected[index];
                    quote!(#parameter: &#call_lifetime #ty)
                });
                let call_arguments = arguments.iter().map(|&index| {
                    let field = &fields[index];
                    quote!(&self.#field)
                });
                let method = if registration.fallible { &take } else { &copy };
                let registered = &initializer_types[i];
                let result = if registration.fallible {
                    quote!(::systasis::__private::check_fallible::<#registered, #output>(::systasis::__private::invoke(#closure)))
                } else {
                    quote!({
                        let __systasis_output: #registered = ::systasis::__private::invoke(#closure);
                        __systasis_output
                    })
                };
                let (impl_generics, _, where_clause) = generics.split_for_impl();
                constructor_functions.push(quote!(
                    pub(super) fn #helper #helper_generics (#(#helper_parameters),*) -> #helper_output #where_clause {
                        let #capture_root = #slot.captures();
                        #result
                    }
                ));
                implementations.push(quote!(
                    impl #impl_generics Generated<#(#selected),*> #where_clause {
                        pub fn #method(&self) -> #output { #helper #turbofish (#(#call_arguments),*) }
                    }
                ));
                continue;
            }
            let others = parameters
                .iter()
                .enumerate()
                .filter(|(j, _)| *j != i)
                .map(|(_, p)| p)
                .collect::<Vec<_>>();
            let lifetime = quote!();
            let reference = quote!();
            let copy_args = parameters
                .iter()
                .enumerate()
                .map(|(j, p)| {
                    if i == j {
                        quote!(#reference ::systasis::__private::CopySlot<__Value>)
                    } else {
                        quote!(#p)
                    }
                })
                .collect::<Vec<_>>();
            let take_args = parameters
                .iter()
                .enumerate()
                .map(|(j, p)| {
                    if i == j {
                        quote!(#reference #slot_type<__Value>)
                    } else {
                        quote!(#p)
                    }
                })
                .collect::<Vec<_>>();
            let fresh_args = parameters
                .iter()
                .enumerate()
                .map(|(j, p)| {
                    if i == j {
                        quote!(::systasis::__private::FreshSlot<__Value>)
                    } else {
                        quote!(#p)
                    }
                })
                .collect::<Vec<_>>();
            if registration.dynamic {
                let interface = &registration.interface;
                let dyn_ref = format_ident!("try_resolve_{snake}_dyn_ref");
                let copy_dyn_ref = format_ident!("resolve_{snake}_dyn_ref");
                implementations.push(quote!(
                    impl<__Value: #interface, #(#others),*> Generated<#(#take_args),*> {
                        pub fn #dyn_ref(&self) -> ::core::result::Result<#read_type<'_, dyn #interface + '_>, ::systasis::__private::Error> {
                            self.#field.try_resolve_ref().map(|guard| #read_type::map(guard, |value| value as &(dyn #interface + '_)))
                        }
                    }
                    impl<__Value: #interface + ::core::marker::Copy, #(#others),*> Generated<#(#copy_args),*> {
                        pub fn #copy_dyn_ref(&self) -> &(dyn #interface + '_) { self.#field.resolve_ref() }
                    }
                ));
            }
            let take_method = (!constructor_borrows.contains(&i)).then(|| quote!(
                pub fn #take(&self) -> ::core::result::Result<__Value,::systasis::__private::Error> { self.#field.try_resolve() }
            ));
            implementations.push(quote!(
                impl<__Value: ::core::default::Default, #(#others),*> Generated<#(#fresh_args),*> {
                    pub fn #copy(&self) -> __Value { self.#field.resolve() }
                }
                impl<#lifetime __Value: ::core::marker::Copy, #(#others),*> Generated<#(#copy_args),*> {
                    pub fn #copy(&self) -> __Value { self.#field.resolve() }
                    pub fn #copy_ref(&self) -> &__Value { self.#field.resolve_ref() }
                    pub fn #clone(&self) -> __Value { self.#field.resolve_clone() }
                }
                impl<#lifetime __Value, #(#others),*> Generated<#(#take_args),*> {
                    pub fn #read(&self) -> ::core::result::Result<#read_type<'_,__Value>,::systasis::__private::Error> { self.#field.try_resolve_ref() }
                    pub fn #write(&self) -> ::core::result::Result<#write_type<'_,__Value>,::systasis::__private::Error> { self.#field.try_resolve_ref_mut() }
                    #take_method
                    pub fn #try_clone(&self) -> ::core::result::Result<__Value,::systasis::__private::Error>
                    where __Value: ::core::clone::Clone { self.#field.try_resolve_clone() }
                }
            ));
        }
        let imports = bindings.imports();
        field_types.push(generic_marker);
        let generic_parameter = parameters.last().unwrap();
        let (alias_parameters, alias_arguments, alias_where) = generics.split_for_impl();
        let mut construction_type: Type = parse_quote!(AppContainer #alias_arguments);
        if let Type::Path(path) = &mut construction_type
            && let PathArguments::AngleBracketed(arguments) =
                &mut path.path.segments.last_mut().unwrap().arguments
        {
            for argument in &mut arguments.args {
                if let GenericArgument::Lifetime(lifetime) = argument
                    && lifetimes
                        .0
                        .iter()
                        .any(|lifted| lifted.ident == lifetime.ident)
                {
                    *lifetime = Lifetime::new("'_", lifetime.span());
                }
            }
        }
        emitted = ::core::option::Option::Some(quote!(
            #[allow(non_snake_case, unused_imports, dead_code)]
            mod __systasis_injected {
                use super::*;
                #(#imports)*
                use ::systasis::__private::CopyFallback as _;
                #(#constants)*
                #(#const_markers)*
                #(#constructor_functions)*
                pub struct Generated<#(#parameters),*> {
                    #(pub(super) #fields: #parameters,)*
                    pub(super) _pin: ::core::marker::PhantomPinned,
                    pub(super) _parameters: ::core::marker::PhantomData<(#marker, #generic_parameter)>,
                }
                #(#implementations)*
                #[allow(type_alias_bounds)]
                pub type Container #alias_parameters #alias_where = Generated<#(#field_types),*>;
            }
            /// The container generated from this module's registration declaration.
            #[allow(type_alias_bounds)]
            pub type AppContainer #alias_parameters #alias_where = __systasis_injected::Container #alias_arguments;
        ));
        let mut initialization = Vec::new();
        let mut capture_initialization = Vec::new();
        for (&index, factory) in &factories {
            let owner = format_ident!(
                "__systasis_capture_owner_{index}",
                span = Span::mixed_site()
            );
            let captures = factory.captures.iter().map(|(name, _)| name);
            let types = factory.captures.iter().map(|(_, ty)| ty);
            capture_initialization.push(quote!(
                let #owner = {
                    let __systasis_input: (#(#types,)*) = (#(#captures,)*);
                    ::systasis::__private::FactorySlot::new(__systasis_input)
                };
            ));
        }
        for i in &order {
            let slot = &slots[*i];
            let value = &registrations[*i].value;
            let policy = &policies[*i];
            let ty = &initializer_types[*i];
            let capture_owner =
                format_ident!("__systasis_capture_owner_{i}", span = Span::mixed_site());
            let skipped_capture = factories
                .contains_key(i)
                .then(|| quote!(::systasis::__private::discard(#capture_owner);));
            let stored = if factories.contains_key(i) {
                quote!(#capture_owner)
            } else if registrations[*i].fresh {
                quote!(::systasis::__private::FreshSlot::<#ty>::new())
            } else {
                quote!({
                    let __systasis_input: #ty = { #value };
                    <::systasis::__private::Policy<#policy, #local_policy> as ::systasis::__private::Select<#ty>>::store(__systasis_input)
                })
            };
            initialization.push(quote!(let (#slot,#systasis_error_ident)=match #systasis_error_ident {
                ::core::option::Option::Some(error)=>{#skipped_capture (::core::option::Option::None,::core::option::Option::Some(error))},
                ::core::option::Option::None=>::systasis::__private::split((|| -> ::core::result::Result<_, #error_ty> {
                    // An empty match coerces into the contextual error type.
                    // It cannot execute: this expression constructs Ok, whose
                    // source error type Infallible has no inhabitants.
                    ::core::result::Result::<_, ::core::convert::Infallible>::Ok(
                        #stored
                    ).map_err(|never| match never {})
                })()),
            };));
        }
        let reverse = order.iter().rev().map(|i| &slots[*i]);
        let pattern = &local.pat;
        let otherwise = initializer
            .diverge
            .as_ref()
            .map(|(_, expression)| quote!(else #expression));
        let checks = requirements.iter().map(|bound| {
            quote!({
                fn __systasis_assert<T: ::core::marker::#bound>(_: &T) {}
                if let ::core::result::Result::Ok(container) = &#systasis_published_ident { __systasis_assert(*container); }
            })
        });
        let macro_path = &registry.mac.path;
        let generated: Block = syn::parse2(quote!({
            #macro_path!(@__systasis_marker);
            #(#policy_checks)*
            #(#validation_calls)*
            #(#capture_initialization)*
            let #systasis_error_ident: ::core::option::Option<#error_ty>=::core::result::Result::<(), ::core::convert::Infallible>::Ok(()).map_err(|never| match never {}).err();
            #(#initialization)*
            let #systasis_result_ident=match #systasis_error_ident {
                ::core::option::Option::None=>{
                    let container: #construction_type = AppContainer {#(#values,)*_pin: ::core::marker::PhantomPinned,_parameters: ::core::marker::PhantomData};
                    ::core::result::Result::Ok(container)
                },
                ::core::option::Option::Some(error)=>{#(::systasis::__private::discard(#reverse);)* ::core::result::Result::Err(error)},
            };
            let (#systasis_value_ident,#systasis_error_ident)=::systasis::__private::split(#systasis_result_ident);
            let #systasis_owner_ident=::core::pin::pin!(#systasis_value_ident);
            let #systasis_published_ident=match (#systasis_owner_ident.as_ref().get_ref(),#systasis_error_ident) {
                (::core::option::Option::Some(container),::core::option::Option::None)=>::core::result::Result::Ok(container),
                (::core::option::Option::None,::core::option::Option::Some(error))=>::core::result::Result::Err(error),
                _=>::core::unreachable!("split result has exactly one occupied branch"),
            };
            #(#checks)*
            let #pattern = #published_expression #otherwise;
        }))?;
        statements.extend(generated.stmts);
    }
    function.block.stmts = statements;
    ::core::result::Result::Ok(quote!(#emitted #function))
}
