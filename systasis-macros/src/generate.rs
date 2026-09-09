use crate::analysis::{Queries, TypeLookup};
use crate::parse::Registrations;
use proc_macro2::Span;
use quote::{ToTokens, format_ident, quote};
use std::collections::{BTreeMap, BTreeSet};
use syn::{ext::IdentExt, visit_mut::VisitMut, *};
struct Lifetimes(Vec<Lifetime>);
struct CallLifetime(Lifetime);

/// Apply namespace suffixes after operation modifiers, and validate the complete
/// public method names rather than only their interface-name portion.
fn namespace_methods(
    implementations: &mut [proc_macro2::TokenStream],
    registration: &crate::parse::Registration,
    known: &mut BTreeMap<String, proc_macro2::TokenStream>,
) -> Result<()> {
    let mut names = BTreeSet::new();
    for implementation in implementations {
        let mut file = syn::parse2::<File>(implementation.clone())?;
        for item in &mut file.items {
            let Item::Impl(implementation) = item else {
                continue;
            };
            let mut aliases = Vec::new();
            for item in &mut implementation.items {
                let ImplItem::Fn(method) = item else {
                    continue;
                };
                if let Some(namespace) = &registration.namespace.0 {
                    method.sig.ident =
                        format_ident!("{}_in_{}", method.sig.ident, namespace.unraw());
                } else {
                    let mut alias = method.clone();
                    alias.sig.ident = format_ident!("{}_in_default", method.sig.ident);
                    names.insert(alias.sig.ident.to_string());
                    aliases.push(ImplItem::Fn(alias));
                }
                names.insert(method.sig.ident.to_string());
            }
            implementation.items.extend(aliases);
        }
        *implementation = file.into_token_stream();
    }
    let interface = &registration.interface;
    let source = if let Some(namespace) = &registration.namespace.0 {
        quote!(#interface in #namespace)
    } else {
        quote!(#interface)
    };
    for name in names {
        if let Some(previous) = known.insert(name.clone(), source.clone()) {
            let mut error = Error::new_spanned(
                &source,
                format!("interfaces generate the same resolver name: {name}"),
            );
            error.combine(Error::new_spanned(previous, "first conflicting interface"));
            return Err(error);
        }
    }
    Ok(())
}
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
            .map(|(i, registration)| (registration.namespace.key(&registration.interface), i))
            .collect::<BTreeMap<_, _>>();
        registrations = registrations
            .into_iter()
            .enumerate()
            .filter_map(|(i, registration)| {
                (winners[&registration.namespace.key(&registration.interface)] == i)
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
            .map(|(i, r)| (r.namespace.key(&r.interface), i))
            .collect::<BTreeMap<_, _>>();
        let mut dependencies = Vec::new();
        let mut constructor_borrows = BTreeSet::new();
        let original_types = registrations
            .iter()
            .map(|r| r.ty.clone())
            .collect::<Vec<_>>();
        let mut dynamic_declarations = Vec::new();
        let dynamic = registrations
            .iter()
            .enumerate()
            .map(|(index, registration)| {
                registration.dynamic.then(|| {
                    let target = crate::dyn_targets::generate(
                        index,
                        &registration.interface,
                        original_generics,
                        &parse_quote!('_),
                    );
                    dynamic_declarations.push(target.declarations);
                    let mut ty = target.ty;
                    if registration.interface.0.len() > 1 {
                        let Type::TraitObject(object) = &mut ty else {
                            unreachable!("dyn target generator returns a trait object type")
                        };
                        for bound in &mut object.bounds {
                            if let TypeParamBound::Trait(bound) = bound {
                                bound
                                    .path
                                    .segments
                                    .insert(0, parse_quote!(__systasis_injected));
                            }
                        }
                    }
                    ty
                })
            })
            .collect::<Vec<_>>();
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
                .map(|&i| registrations[i].namespace.key(&registrations[i].interface))
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
            &dynamic,
        );
        let mut runtime_queries = crate::wiring::replacements(
            &registrations,
            &transitive,
            false,
            local_policy,
            &turbofish,
            &dynamic,
        );
        for &index in &constructor_borrows {
            build_queries.remove(&(index, "try_resolve".into()));
            runtime_queries.remove(&(index, "try_resolve".into()));
            build_queries.remove(&(index, "resolve_unchecked".into()));
            runtime_queries.remove(&(index, "resolve_unchecked".into()));
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
        let mut method_names = BTreeMap::<String, proc_macro2::TokenStream>::new();
        for (i, registration) in registrations.iter().enumerate() {
            let first_implementation = implementations.len();
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
                namespace_methods(
                    &mut implementations[first_implementation..],
                    registration,
                    &mut method_names,
                )?;
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
                let target = dynamic[i].as_ref().unwrap_or_else(|| {
                    unreachable!("every opted-in registration has a generated dyn target")
                });
                let dyn_lifetime = (0..)
                    .map(|suffix| {
                        Lifetime::new(&format!("'__systasis_dyn_{suffix}"), Span::mixed_site())
                    })
                    .find(|candidate| {
                        !generics
                            .lifetimes()
                            .any(|parameter| parameter.lifetime.ident == candidate.ident)
                    })
                    .unwrap_or_else(|| {
                        unreachable!(
                            "the finite lifetime parameter list cannot exhaust identifier suffixes"
                        )
                    });
                let mut target = target.clone();
                CallLifetime(dyn_lifetime.clone()).visit_type_mut(&mut target);
                let dyn_ref = format_ident!("try_resolve_{snake}_dyn_ref");
                let copy_dyn_ref = format_ident!("resolve_{snake}_dyn_ref");
                let mut dyn_generics = generics.clone();
                let dyn_value = crate::dyn_targets::value_parameter(&generics);
                dyn_generics.params.push(parse_quote!(#dyn_value));
                for parameter in &others {
                    if Some(*parameter) != parameters.last() {
                        dyn_generics.params.push(parse_quote!(#parameter));
                    }
                }
                // Mapping guards requires a projection valid for every input
                // reference lifetime. First recovering the declared type keeps
                // its lifetime relations visible, unlike an erased __Value.
                // Actual slots store exactly that type, so Borrow<T> for T
                // supplies this bound without a caller-written implementation.
                let dyn_bounds: [WherePredicate; 2] = [
                    parse_quote!(#dyn_value: ::core::borrow::Borrow<#ty>),
                    parse_quote!(#ty: #interface),
                ];
                dyn_generics
                    .make_where_clause()
                    .predicates
                    .extend(dyn_bounds);
                let (dyn_parameters, _, dyn_where) = dyn_generics.split_for_impl();
                let mut copy_dyn_generics = dyn_generics.clone();
                copy_dyn_generics
                    .make_where_clause()
                    .predicates
                    .push(parse_quote!(#dyn_value: ::core::marker::Copy));
                let (copy_dyn_parameters, _, copy_dyn_where) = copy_dyn_generics.split_for_impl();
                let mut dyn_take_args = take_args.clone();
                let mut dyn_copy_args = copy_args.clone();
                dyn_take_args[i] = quote!(#slot_type<#dyn_value>);
                dyn_copy_args[i] = quote!(::systasis::__private::CopySlot<#dyn_value>);
                *dyn_take_args.last_mut().unwrap() = generic_marker.clone();
                *dyn_copy_args.last_mut().unwrap() = generic_marker.clone();
                implementations.push(quote!(
                    impl #dyn_parameters Generated<#(#dyn_take_args),*> #dyn_where {
                        pub fn #dyn_ref<#dyn_lifetime>(&#dyn_lifetime self) -> ::core::result::Result<#read_type<#dyn_lifetime, #target>, ::systasis::__private::Error> {
                            self.#field.try_resolve_ref().map(|guard| #read_type::map(guard, |value| <#dyn_value as ::core::borrow::Borrow<#ty>>::borrow(value) as &(#target)))
                        }
                    }
                    impl #copy_dyn_parameters Generated<#(#dyn_copy_args),*> #copy_dyn_where {
                        pub fn #copy_dyn_ref<#dyn_lifetime>(&#dyn_lifetime self) -> &#dyn_lifetime (#target) {
                            <#dyn_value as ::core::borrow::Borrow<#ty>>::borrow(self.#field.resolve_ref())
                        }
                    }
                ));
            }
            let take_method = (!constructor_borrows.contains(&i)).then(|| quote!(
                pub fn #take(&self) -> ::core::result::Result<__Value,::systasis::__private::Error> { self.#field.try_resolve() }
            ));
            let unchecked_methods = (cfg!(feature = "resolve_unchecked") && !registration.fresh)
                .then(|| {
                    let take = format_ident!("resolve_{snake}_unchecked");
                    let read = format_ident!("resolve_{snake}_ref_unchecked");
                    let write = format_ident!("resolve_{snake}_ref_mut_unchecked");
                    let owned = (!constructor_borrows.contains(&i)).then(|| {
                        quote!(
                            /// Takes the available value without returning access errors.
                            /// # Safety
                            /// The value must be present and exclusive acquisition must succeed.
                            pub unsafe fn #take(&self) -> __Value {
                                // SAFETY: the caller guarantees the slot's preconditions.
                                unsafe { self.#field.resolve_unchecked() }
                            }
                        )
                    });
                    quote!(
                        #owned
                        /// Borrows the available value and retains its shared guard.
                        /// # Safety
                        /// The value must be present and shared acquisition must succeed.
                        pub unsafe fn #read(&self) -> #read_type<'_, __Value> {
                            // SAFETY: the caller guarantees the slot's preconditions.
                            unsafe { self.#field.resolve_ref_unchecked() }
                        }
                        /// Mutably borrows the available value and retains its exclusive guard.
                        /// # Safety
                        /// The value must be present and exclusive acquisition must succeed.
                        pub unsafe fn #write(&self) -> #write_type<'_, __Value> {
                            // SAFETY: the caller guarantees the slot's preconditions.
                            unsafe { self.#field.resolve_ref_mut_unchecked() }
                        }
                    )
                });
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
                    #unchecked_methods
                    pub fn #try_clone(&self) -> ::core::result::Result<__Value,::systasis::__private::Error>
                    where __Value: ::core::clone::Clone { self.#field.try_resolve_clone() }
                }
            ));
            namespace_methods(
                &mut implementations[first_implementation..],
                registration,
                &mut method_names,
            )?;
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
                #(#dynamic_declarations)*
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
    let mut definitions: syn::File = syn::parse2(quote!(#emitted))?;
    for item in &mut definitions.items {
        if let syn::Item::Mod(module) = item {
            crate::rebase::generated_module(module);
        }
    }
    ::core::result::Result::Ok(quote!(#definitions #function))
}
