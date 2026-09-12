use crate::analysis::{Queries, TypeLookup};
use crate::parse::Registrations;
use proc_macro2::Span;
use quote::{ToTokens, format_ident, quote};
use std::collections::{BTreeMap, BTreeSet};
use syn::{ext::IdentExt, spanned::Spanned, visit_mut::VisitMut, *};
struct Lifetimes(Vec<Lifetime>);
struct CallLifetime(Lifetime);
struct SourceLifetimes<'a>(&'a [Lifetime]);
struct NativeContext<'a>(&'a Ident, usize);

impl VisitMut for SourceLifetimes<'_> {
    fn visit_lifetime_mut(&mut self, lifetime: &mut Lifetime) {
        if lifetime.ident == "__systasis_call"
            || self.0.iter().any(|lifted| lifted.ident == lifetime.ident)
        {
            *lifetime = Lifetime::new("'_", lifetime.span());
        }
    }
}

impl VisitMut for NativeContext<'_> {
    // Nested items have their own receiver and cannot capture this invocation's
    // context. In particular, keep `self` in caller-authored impl methods intact.
    fn visit_item_mut(&mut self, _: &mut Item) {}

    fn visit_expr_mut(&mut self, expression: &mut Expr) {
        // Direct queries need only their selected child's descriptor. Rebuild
        // the complete tuple only when forwarding it to another constructor.
        if let Expr::Field(field) = expression
            && let Member::Unnamed(position) = &field.member
            && matches!(&*field.base, Expr::Field(children)
                if matches!(&children.member, Member::Named(name) if name == "_children")
                && matches!(&*children.base, Expr::Path(path) if path.path.is_ident("self")))
        {
            let context = self.0;
            *expression = parse_quote!(#context._children.#position.descriptor());
            return;
        }
        let child = matches!(expression, Expr::Field(field)
            if matches!(&field.member, Member::Named(name) if name == "_children")
            && matches!(&*field.base, Expr::Path(path) if path.path.is_ident("self")));
        syn::visit_mut::visit_expr_mut(self, expression);
        if child {
            let context = self.0;
            let descriptors = (0..self.1).map(|index| {
                let position = Index::from(index);
                quote!(#context._children.#position.descriptor())
            });
            *expression = parse_quote!(&(#(#descriptors,)*));
        }
    }

    fn visit_expr_path_mut(&mut self, expression: &mut ExprPath) {
        if expression.path.is_ident("self") {
            let context = self.0;
            expression.path = parse_quote!(#context);
        }
        syn::visit_mut::visit_expr_path_mut(self, expression);
    }
}
struct ProjectionLifetimes<'a> {
    lifted: &'a [Lifetime],
    predicates: Vec<WherePredicate>,
}
impl VisitMut for ProjectionLifetimes<'_> {
    fn visit_type_path_mut(&mut self, path: &mut TypePath) {
        if let Some(qualified) = &path.qself
            && path
                .path
                .segments
                .iter()
                .any(|segment| segment.ident == "scoped")
            && path
                .path
                .segments
                .iter()
                .any(|segment| segment.ident == "Registered" || segment.ident == "DynRegistered")
            && let Some(segment) = path.path.segments.last()
            && let PathArguments::AngleBracketed(arguments) = &segment.arguments
            && let Some(GenericArgument::Lifetime(lifetime)) = arguments.args.first()
            && self
                .lifted
                .iter()
                .any(|lifted| lifted.ident == lifetime.ident)
        {
            let owner = &qualified.ty;
            self.predicates.push(parse_quote!(#owner: #lifetime));
        }
        syn::visit_mut::visit_type_path_mut(self, path);
    }
}

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
    fn visit_type_reference_mut(&mut self, reference: &mut TypeReference) {
        reference
            .lifetime
            .get_or_insert_with(|| Lifetime::new("'_", reference.and_token.span));
        syn::visit_mut::visit_type_reference_mut(self, reference);
    }

    // Elision within a function signature introduces bound lifetimes, not
    // captures of the enclosing container. Preserve those signatures intact.
    fn visit_type_fn_ptr_mut(&mut self, _: &mut TypeFnPtr) {}
    fn visit_parenthesized_generic_arguments_mut(&mut self, _: &mut ParenthesizedGenericArguments) {
    }

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
    let mut opaque_definitions = Vec::new();
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
        let crate::child::Input {
            registrations: Registrations(mut registrations),
            mut children,
        } = syn::parse2(registry.mac.tokens.clone())?;
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
        let mut child_borrows = Vec::new();
        for child in &children {
            if let Some(namespace) = registrations
                .iter()
                .filter_map(|r| r.namespace.0.as_ref())
                .find(|namespace| namespace.unraw() == child.name.unraw())
            {
                let mut error =
                    Error::new_spanned(&child.name, "child name conflicts with a local namespace");
                error.combine(Error::new_spanned(
                    namespace,
                    "local namespace is declared here",
                ));
                return Err(error);
            }
        }
        let mut child_calls = Vec::new();
        for registration in &mut registrations {
            let mut queries = crate::child_queries::Queries {
                children: &children,
                error: None,
                borrowed: Vec::new(),
                calls: Vec::new(),
                in_constructor: false,
            };
            queries.visit_type_mut(&mut registration.ty);
            queries.visit_expr_mut(&mut registration.value);
            // Temporary initializer reads do not remove public ownership methods.
            queries.borrowed.clear();
            queries.calls.clear();
            if let Some(constructor) = &mut registration.constructor {
                queries.in_constructor = true;
                queries.visit_expr_closure_mut(constructor);
            }
            child_borrows.extend(queries.borrowed);
            child_calls.push(queries.calls);
            if let Some(error) = queries.error {
                return Err(error);
            }
        }
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
        let consumed = crate::scopegen::consumed_dependencies(&registrations, &indices, &order)?;
        crate::scopegen::propagate_child_calls(&registrations, &indices, &order, &mut child_calls)?;
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
        let children_ident = crate::wiring::children_ident();
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
                    crate::captures::prepare(closure, &bindings, &capture_root),
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
        let mut alias_policies = Vec::new();
        let mut policy_checks = Vec::new();
        let mut lifetimes = Lifetimes(Vec::new());
        for child in &mut children {
            child
                .ty
                .lifetime
                .get_or_insert_with(|| Lifetime::new("'_", Span::mixed_site()));
            lifetimes.visit_type_reference_mut(&mut child.ty);
        }
        for (i, registration) in registrations.iter_mut().enumerate() {
            let original = &registration.ty;
            let flag = &flags[i];
            let mut projected = false;
            if crate::generic_policy::depends_on_generics(original, original_generics) {
                let copy =
                    crate::generic_policy::has_explicit_copy_bound(original, original_generics);
                let inherited = (!copy)
                    .then(|| crate::child_queries::projected_policy(original, local_policy))
                    .flatten();
                projected = inherited.is_some();
                policies.push(inherited.unwrap_or_else(
                    || quote!(::systasis::__private::Policy<#copy, #local_policy>),
                ));
                if !copy && !projected && !registration.fresh {
                    policy_checks.push(quote!({
                        use ::systasis::__private::DetectCopy as _;
                        ::systasis::__private::verify_generic_fallback(
                            (&&::systasis::__private::Pick::<#original>::NEW).evidence()
                        );
                    }));
                }
            } else {
                policies.push(quote!(::systasis::__private::Policy<{__systasis_injected::#flag}, #local_policy>));
                constants.push(quote!(pub(super) const #flag: bool = ::systasis::__private::Pick::<#original>::IS_COPY;));
            }
            lifetimes.visit_type_mut(&mut registration.ty);
            alias_policies.push(if projected {
                crate::child_queries::projected_policy(&registration.ty, local_policy)
                    .unwrap_or_else(|| {
                        unreachable!(
                            "lifetime lifting preserves the child projection's path structure"
                        )
                    })
            } else {
                policies[i].clone()
            });
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
        let mut selected = registrations
            .iter()
            .enumerate()
            .map(|(i, r)| {
                let ty = &r.ty;
                let policy = &alias_policies[i];
                if let Some(types) = capture_types.get(&i) {
                    quote!(::systasis::__private::FactorySlot<(#(#types,)*)>)
                } else if r.fresh {
                    quote!(::systasis::__private::FreshSlot<#ty>)
                } else {
                    quote!(<#policy as ::systasis::__private::Select<#ty>>::Slot)
                }
            })
            .collect::<Vec<_>>();
        let mut generics = function.sig.generics.clone();
        for lifetime in &lifetimes.0 {
            generics.params.insert(0, parse_quote!(#lifetime));
        }
        // Generated GAT scope projections must retain the well-formedness
        // relation already present in the caller's shared child reference.
        for child in &children {
            let ty = &child.ty.elem;
            let lifetime = &child.ty.lifetime;
            generics
                .make_where_clause()
                .predicates
                .push(parse_quote!(#ty: #lifetime));
        }
        let mut projections = ProjectionLifetimes {
            lifted: &lifetimes.0,
            predicates: Vec::new(),
        };
        for registration in &registrations {
            projections.visit_type_mut(&mut registration.ty.clone());
        }
        if !projections.predicates.is_empty() {
            generics
                .make_where_clause()
                .predicates
                .extend(projections.predicates);
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
        // Stored payload and child types already occur in the storage/scope aliases.
        // Retain their spelling here so projection normalization does not make
        // scope metadata appear to expose a type absent from its implementing type.
        let stored_types = registrations
            .iter()
            .filter(|registration| !registration.fresh)
            .map(|registration| &registration.ty);
        let child_marker_types = children.iter().map(|child| &child.ty.elem);
        let generic_marker =
            quote!(fn() -> (#(#marker_types,)* #(#stored_types,)* #(#child_marker_types,)*));
        let mut capture_records = Vec::new();
        let mut capture_record_names = BTreeMap::new();
        let (capture_parameters, capture_arguments, capture_where) = generics.split_for_impl();
        for (&index, types) in &capture_types {
            if factories[&index].native {
                let name = format_ident!("__NativeFactory{index}");
                let (_, native_arguments, _) = original_generics.split_for_impl();
                selected[index] = quote!(::systasis::__private::FactorySlot<__systasis_injected::#name #native_arguments>);
                continue;
            }
            if !factories[&index].requires_record {
                continue;
            }
            let name = format_ident!("__CaptureRecord{index}");
            // Keep private projection outputs inside private fields rather
            // than exposing their normalization through scoped trait impls.
            // The record owns the same selected captures as the ordinary tuple.
            capture_records.push(quote! {
                pub struct #name #capture_parameters (
                    #(pub(super) #types,)*
                    pub(super) ::core::marker::PhantomData<fn() -> (#(#marker_types,)*)>
                ) #capture_where;
            });
            selected[index] = quote!(::systasis::__private::FactorySlot<__systasis_injected::#name #capture_arguments>);
            capture_record_names.insert(index, name);
        }
        let child_masks = children
            .iter()
            .enumerate()
            .map(|(index, _)| {
                child_borrows
                    .iter()
                    .filter(|(child, _)| *child == index)
                    .fold(
                        quote!(::systasis::scoped::mask::Empty),
                        |tail, (_, mask)| quote!(::systasis::scoped::mask::Union<#mask, #tail>),
                    )
            })
            .collect::<Vec<_>>();
        let child_types = children
            .iter()
            .zip(&child_masks)
            .map(|(child, mask)| {
                let lifetime =
                    child.ty.lifetime.as_ref().unwrap_or_else(|| {
                        unreachable!("child lifetime was inserted before lifting")
                    });
                let ty = &child.ty.elem;
                quote!(<#ty as ::systasis::scoped::AsScope<#mask>>::Scope<#lifetime>)
            })
            .collect::<Vec<_>>();
        let child_tuple = quote!((#(#child_types,)*));
        let nominal_children = !children.is_empty();
        let child_arguments = generics.params.iter().map(|parameter| match parameter {
            GenericParam::Type(parameter) => {
                let name = &parameter.ident;
                quote!(#name)
            }
            GenericParam::Lifetime(parameter) => {
                let name = &parameter.lifetime;
                quote!(#name)
            }
            GenericParam::Const(parameter) => {
                let name = &parameter.ident;
                quote!(#name)
            }
        });
        let child_storage = if !nominal_children {
            child_tuple.clone()
        } else {
            quote!(__systasis_injected::__StoredChildren<#(#child_arguments,)* #generic_marker>)
        };
        let child_storage_definitions = nominal_children.then(|| {
            crate::child_codegen::stored_children(
                &generics,
                &child_tuple,
                &quote!(fn() -> (#(#marker_types,)*)),
                &generic_marker,
            )
        });
        let stored_children_ref = if !nominal_children {
            quote!(&self._children)
        } else {
            quote!(&self._children.inner)
        };
        selected.push(child_storage.clone());
        selected.push(generic_marker.clone());
        let parameters = (0..=registrations.len() + 1)
            .map(|i| format_ident!("__Slot{i}"))
            .collect::<Vec<_>>();
        let mut field_types = Vec::new();
        let mut values = Vec::new();
        let mut implementations = Vec::new();
        let mut scoped_implementations = Vec::new();
        let descriptor = crate::scopegen::descriptor(&parameters, &children);
        let child_forwarding = crate::child_codegen::forwarding(&parameters, &children);
        let namespace_forwarding = crate::child_codegen::namespaces(&parameters, &registrations);
        let mut key_declarations = Vec::new();
        let scope_keys = registrations
            .iter()
            .enumerate()
            .map(|(index, registration)| {
                let (key, path, restriction) = crate::scopegen::keys(registration);
                let key_name = format_ident!("__SystasisKey{index}");
                let path_name = format_ident!("__SystasisPath{index}");
                let restriction_name = format_ident!("__SystasisRestrictionKey{index}");
                key_declarations.push(quote!(
                    pub type #key_name = #key;
                    pub type #path_name = #path;
                    pub type #restriction_name = #restriction;
                ));
                (
                    parse_quote!(#key_name),
                    parse_quote!(#path_name),
                    parse_quote!(#restriction_name),
                )
            })
            .collect::<Vec<(Type, Type, Type)>>();
        let mut constructor_functions = Vec::new();
        let mut native_initializers = BTreeMap::new();
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
            // Carry the caller's child references into this proof function:
            // their well-formed input types imply nested outlives relations
            // that a where-clause alone does not make available in the body.
            let checked_children = children.iter().map(|child| &child.ty);
            let checked_child_values = children.iter().map(|child| &child.name);
            constructor_functions.push(quote!(
                pub(super) fn #check #checked_parameters (_: ::core::marker::PhantomData<#ty>, _: (#(#checked_children,)*)) #checked_where {}
            ));
            let original = &initializer_types[i];
            validation_calls.push(quote!(__systasis_injected::#check #turbofish (::core::marker::PhantomData::<#original>, (#(#checked_child_values,)*));));
            let field = &fields[i];
            let slot = &slots[i];
            let storage = &selected[i];
            field_types.push(storage.clone());
            values.push(quote!(#field: #slot.unwrap_or_else(|| ::core::unreachable!("successful build initialized every slot"))));
            let mut names = registration
                .interface
                .0
                .iter()
                .map(|path| {
                    path.segments
                        .last()
                        .unwrap_or_else(|| {
                            unreachable!(
                                "InterfaceGroup parses Rust paths with at least one segment"
                            )
                        })
                        .ident
                        .unraw()
                        .to_string()
                })
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
            let (scope_key, scope_path, scope_mask_key) = &scope_keys[i];
            let consumed_keys = consumed[i]
                .iter()
                .map(|&index| scope_keys[index].2.clone())
                .collect::<Vec<_>>();
            let scope_entry = crate::scopegen::Entry {
                registration,
                snake: &snake,
                key: scope_key,
                path: scope_path,
                own_mask_key: scope_mask_key,
                consumed_keys: &consumed_keys,
                child_calls: &child_calls[i],
                policy: &alias_policies[i],
            };
            scoped_implementations.push(crate::scopegen::value_metadata(
                &scope_entry,
                &parameters,
                i,
                &generics,
                &selected,
                &initializer_types[i],
                dynamic[i].as_ref(),
            ));
            let read = format_ident!("try_resolve_{snake}_ref");
            let write = format_ident!("try_resolve_{snake}_ref_mut");
            let take = format_ident!("try_resolve_{snake}");
            let copy = format_ident!("resolve_{snake}");
            let copy_ref = format_ident!("resolve_{snake}_ref");
            let clone = format_ident!("resolve_{snake}_clone");
            let try_clone = format_ident!("try_resolve_{snake}_clone");
            if let Some(factory) = factories.get(&i) {
                let helper = format_ident!("construct_{i}");
                let context = format_ident!("__ConstructorContext{i}");
                let output = match &factory.closure.output {
                    ReturnType::Default => initializer_types[i].clone(),
                    ReturnType::Type(_, ty) => (**ty).clone(),
                };
                let closure = &factory.closure;
                let call_lifetime = Lifetime::new("'__systasis_call", Span::mixed_site());
                // A descriptor tuple may be temporary while its backing slots
                // remain borrowed by the returned value.
                let children_lifetime =
                    Lifetime::new("'__systasis_children_call", Span::mixed_site());
                let mut helper_output = output.clone();
                CallLifetime(call_lifetime.clone()).visit_type_mut(&mut helper_output);
                let mut helper_generics = generics.clone();
                helper_generics
                    .params
                    .insert(0, parse_quote!(#call_lifetime));
                helper_generics
                    .params
                    .insert(0, parse_quote!(#children_lifetime));
                let arguments = crate::wiring::arguments(i, &transitive);
                // The context exists only for this invocation. Bound its
                // borrowed fields, not unrelated original generic parameters.
                let authored_bounds = helper_generics
                    .type_params_mut()
                    .filter_map(|parameter| {
                        let bounds = ::core::mem::take(&mut parameter.bounds);
                        let name = &parameter.ident;
                        (!bounds.is_empty()).then(|| parse_quote!(#name: #bounds))
                    })
                    .collect::<Vec<WherePredicate>>();
                helper_generics
                    .make_where_clause()
                    .predicates
                    .extend(authored_bounds);
                helper_generics
                    .make_where_clause()
                    .predicates
                    .push(parse_quote!(#child_tuple: #call_lifetime));
                for child in &children {
                    let ty = &child.ty;
                    helper_generics
                        .make_where_clause()
                        .predicates
                        .push(parse_quote!(#ty: #call_lifetime));
                }
                for &index in &arguments {
                    let ty = &selected[index];
                    helper_generics
                        .make_where_clause()
                        .predicates
                        .push(parse_quote!(#ty: #call_lifetime));
                }
                let helper_parameters = arguments
                    .iter()
                    .map(|&index| {
                        let parameter = &slots[index];
                        let ty = &selected[index];
                        quote!(#parameter: &#call_lifetime #ty)
                    })
                    .collect::<Vec<_>>();
                // Native closures receive dependencies, never their own
                // closure slot: including that slot would make the opaque
                // closure type occur recursively in its own Fn argument.
                let context_arguments = arguments
                    .iter()
                    .copied()
                    .filter(|&index| !factory.native || index != i)
                    .collect::<Vec<_>>();
                let context_fields = context_arguments.iter().map(|&index| &slots[index]);
                let context_slot_parameters = context_arguments
                    .iter()
                    .map(|&index| format_ident!("__ContextSlot{index}"))
                    .collect::<Vec<_>>();
                let context_slot_fields = context_arguments
                    .iter()
                    .zip(&context_slot_parameters)
                    .map(|(&index, parameter)| {
                        let field = &slots[index];
                        quote!(#field: &#call_lifetime #parameter)
                    });
                let context_slot_arguments =
                    context_arguments.iter().map(|&index| &selected[index]);
                let context_marker = if factory.native {
                    quote!(())
                } else {
                    quote!(fn() -> (#(#marker_types,)*))
                };
                let context_type = quote!(#context<#children_lifetime, #call_lifetime, #(#context_slot_arguments,)* #child_tuple, #context_marker>);
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
                let (context_parameters, _, context_where) = helper_generics.split_for_impl();
                if factory.native {
                    let native_name = format_ident!("__NativeFactory{i}");
                    let opaque_name = format_ident!("__NativeClosure{i}");
                    let (native_parameters, native_arguments, native_where) =
                        original_generics.split_for_impl();
                    // Avoid a scope GAT projection in the higher-ranked Fn
                    // argument: it can impose 'static on invariant child data.
                    // Name backing types directly, preserving each child's mask.
                    let native_child_types =
                        children.iter().zip(&child_masks).map(|(child, mask)| {
                            let lifetime = child.ty.lifetime.as_ref().unwrap_or_else(|| {
                                unreachable!("Lifetimes::visit_type_reference_mut assigned every child's outer borrow before constructor generation")
                            });
                            let lifetime = if lifetimes
                                .0
                                .iter()
                                .any(|lifted| lifted.ident == lifetime.ident)
                            {
                                // Elided outer borrows may shorten per call;
                                // authored/invariant payload lifetimes must not.
                                &call_lifetime
                            } else {
                                lifetime
                            };
                            let ty = &child.ty.elem;
                            quote!(::systasis::scoped::BorrowedContext<#lifetime, #ty, #mask>)
                        });
                    let native_children = quote!((#(#native_child_types,)*));
                    let native_slots = context_arguments.iter().map(|&index| &selected[index]);
                    let native_context_type = quote!(#context<#call_lifetime, #(#native_slots,)* #native_children, #context_marker>);
                    let context_ident = Ident::new("__systasis_context", Span::mixed_site());
                    let source_slots = context_arguments.iter().map(|&index| &selected[index]);
                    let mut source_context: Type = parse2(
                        quote!(__systasis_injected::#context<'_, #(#source_slots,)* #native_children, _>),
                    )?;
                    SourceLifetimes(&lifetimes.0).visit_type_mut(&mut source_context);
                    // The HRTB is established by the opaque alias. An explicit
                    // context annotation makes the source closure independently
                    // valid before coercion, including returned dependency guards.
                    let mut native_closure = closure.clone();
                    native_closure
                        .inputs
                        .push(Pat::Type(parse_quote!(#context_ident: #source_context)));
                    NativeContext(&context_ident, children.len())
                        .visit_expr_mut(&mut native_closure.body);
                    let body = &native_closure.body;
                    native_closure.body = if registration.fallible {
                        parse_quote!({ ::systasis::__private::check_fallible::<#registered, #output>({ #body }) })
                    } else {
                        parse_quote!({ let __systasis_output: #registered = { #body }; __systasis_output })
                    };
                    // A parameterless opaque alias cannot name a captured
                    // local lifetime. State that existing restriction at a
                    // library source location containing its remedy. Keep the
                    // opaque annotation on the temporary: without it, closures
                    // returning dependency guards lose their expected HRTB.
                    let checked_closure = if original_generics.params.is_empty() {
                        let source_span = registration.constructor.as_ref().map_or_else(
                            || registration.ty.span(),
                            |constructor| constructor.inputs_begin.span,
                        );
                        quote::quote_spanned!(source_span=> {
                            let __systasis_native_closure: __systasis_injected::#opaque_name #native_arguments = #native_closure;
                            ::systasis::__private::check_native_constructor_captures(&__systasis_native_closure);
                            __systasis_native_closure
                        })
                    } else {
                        quote!(#native_closure)
                    };
                    native_initializers.insert(
                        i,
                        quote!(__systasis_injected::#native_name {
                            closure: #checked_closure,
                        }),
                    );
                    opaque_definitions.push(quote!(__systasis_injected::#opaque_name));
                    let native_context_fields = context_arguments
                        .iter()
                        .zip(&context_slot_parameters)
                        .map(|(&index, parameter)| {
                            let field = &slots[index];
                            quote!(pub(super) #field: &#call_lifetime #parameter)
                        });
                    let native_child_values = (0..children.len()).map(|index| {
                        let position = Index::from(index);
                        quote!(::systasis::scoped::BorrowContext::borrow_context(&#children_ident.#position))
                    });
                    // The phantom borrow supplies Children: 'call even when
                    // an authored child borrow has its own longer lifetime.
                    // No borrow of the temporary context is returned.
                    constructor_functions.push(quote!(
                        pub(super) struct #context<#call_lifetime, #(#context_slot_parameters,)* __ContextChildren, __ContextMarker> {
                            #(#native_context_fields,)*
                            pub(super) _children: __ContextChildren,
                            pub(super) _marker: ::core::marker::PhantomData<(&#call_lifetime __ContextChildren, __ContextMarker)>,
                        }
                        pub(super) type #opaque_name #native_parameters #native_where = impl for<#call_lifetime> Fn(#native_context_type) -> #helper_output;
                        // The named wrapper prevents a private constructor
                        // result from leaking through the public container alias.
                        pub struct #native_name #native_parameters #native_where {
                            pub(super) closure: #opaque_name #native_arguments,
                        }
                        pub(super) fn #helper #context_parameters (#(#helper_parameters,)* #children_ident: &#children_lifetime #child_tuple) -> #helper_output #context_where {
                            let context: #native_context_type = #context { #(#context_fields,)* _children: (#(#native_child_values,)*), _marker: ::core::marker::PhantomData };
                            (#slot.captures().closure)(context)
                        }
                    ));
                } else {
                    constructor_functions.push(quote!(
                    // Keyword-based field access cannot be shadowed by imports
                    // within the caller's constructor body.
                    struct #context<#children_lifetime, #call_lifetime, #(#context_slot_parameters,)* __ContextChildren, __ContextMarker> {
                        #(#context_slot_fields,)*
                        _children: &#children_lifetime __ContextChildren,
                        _marker: ::core::marker::PhantomData<__ContextMarker>,
                    }
                    impl #context_parameters #context_type #context_where {
                        fn invoke(self) -> #helper_output {
                            let #capture_root = self.#slot.captures();
                            #result
                        }
                    }
                    pub(super) fn #helper #context_parameters (#(#helper_parameters,)* #children_ident: &#children_lifetime #child_tuple) -> #helper_output #context_where {
                        let context: #context_type = #context { #(#context_fields,)* _children: #children_ident, _marker: ::core::marker::PhantomData };
                        context.invoke()
                    }
                    ));
                }
                implementations.push(quote!(
                    impl #impl_generics Generated<#(#selected),*> #where_clause {
                        pub fn #method(&self) -> #output { #helper #turbofish (#(#call_arguments,)* #stored_children_ref) }
                    }
                ));
                namespace_methods(
                    &mut implementations[first_implementation..],
                    registration,
                    &mut method_names,
                )?;
                scoped_implementations.push(crate::scopegen::resolvers(
                    &implementations[first_implementation..],
                    &scope_entry,
                )?);
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
                *dyn_take_args.last_mut().unwrap_or_else(|| {
                    unreachable!(
                        "take_args maps every parameter, including the final generic marker"
                    )
                }) = generic_marker.clone();
                *dyn_copy_args.last_mut().unwrap_or_else(|| {
                    unreachable!(
                        "copy_args maps every parameter, including the final generic marker"
                    )
                }) = generic_marker.clone();
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
            scoped_implementations.push(crate::scopegen::resolvers(
                &implementations[first_implementation..],
                &scope_entry,
            )?);
        }
        let imports = bindings.imports();
        let capture_projection_helpers = bindings.projection_helpers();
        field_types.push(child_storage);
        field_types.push(generic_marker);
        let child_parameter = &parameters[registrations.len()];
        let generic_parameter = parameters.last().unwrap_or_else(|| {
            unreachable!(
                "parameters includes child and generic-marker entries even with no registrations"
            )
        });
        let (alias_parameters, alias_arguments, alias_where) = generics.split_for_impl();
        let mut child_aliases = Vec::new();
        let mut child_exports = Vec::new();
        let mut child_accessors = Vec::new();
        let mut child_values = Vec::new();
        for (index, (child, ty)) in children.iter().zip(&child_types).enumerate() {
            let name = &child.name;
            if method_names.contains_key(&name.unraw().to_string()) {
                return Err(Error::new_spanned(
                    name,
                    "child path conflicts with a generated resolver name",
                ));
            }
            let alias = format_ident!("__SystasisChild{index}");
            let position = Index::from(index);
            let alias_type: Type = syn::parse2(ty.clone())?;
            let child_generics = crate::child::scope_generics(&generics, &alias_type);
            let (child_parameters, _, child_where) = child_generics.split_for_impl();
            child_aliases.push(quote!(#[allow(type_alias_bounds)] pub type #alias #child_parameters #child_where = #ty;));
            child_exports.push(quote!(pub mod #name { pub use super::__systasis_injected::#alias as SubContainer; }));
            child_accessors
                .push(quote!(pub fn #name(&self) -> &#ty { &self._children.inner.#position }));
            let mask = &child_masks[index];
            child_values.push(quote!(::systasis::scoped::AsScope::<#mask>::scope(#name)));
        }
        let mut construction_type: Type = parse_quote!(AppContainer #alias_arguments);
        if let Type::Path(path) = &mut construction_type
            && let PathArguments::AngleBracketed(arguments) = &mut path
                .path
                .segments
                .last_mut()
                .unwrap_or_else(|| {
                    unreachable!("construction_type was just parsed from the AppContainer path")
                })
                .arguments
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
                #capture_projection_helpers
                #(#capture_records)*
                #child_storage_definitions
                use ::systasis::__private::CopyFallback as _;
                #(#constants)*
                #(#const_markers)*
                #(#dynamic_declarations)*
                #(#constructor_functions)*
                #(#key_declarations)*
                pub struct Generated<#(#parameters),*> {
                    #(pub(super) #fields: #parameters,)*
                    pub(super) _children: #child_parameter,
                    pub(super) _pin: ::core::marker::PhantomPinned,
                    pub(super) _parameters: ::core::marker::PhantomData<(#marker, #generic_parameter)>,
                }
                #(#implementations)*
                impl #alias_parameters Generated<#(#selected),*> #alias_where { #(#child_accessors)* }
                #(#child_aliases)*
                #descriptor
                #child_forwarding
                #namespace_forwarding
                #(#scoped_implementations)*
                #[allow(type_alias_bounds)]
                pub type Container #alias_parameters #alias_where = Generated<#(#field_types),*>;
            }
            /// The container generated from this module's registration declaration.
            #[allow(type_alias_bounds)]
            pub type AppContainer #alias_parameters #alias_where = __systasis_injected::Container #alias_arguments;
            #(#child_exports)*
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
            let capture_value = if let Some(initializer) = native_initializers.get(&index) {
                initializer.clone()
            } else if let Some(name) = capture_record_names.get(&index) {
                quote!(__systasis_injected::#name(#(#captures,)* ::core::marker::PhantomData))
            } else {
                quote!({ let __systasis_input: (#(#types,)*) = (#(#captures,)*); __systasis_input })
            };
            capture_initialization.push(quote!(
                let #owner = {
                    let __systasis_input = #capture_value;
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
                    <#policy as ::systasis::__private::Select<#ty>>::store(__systasis_input)
                })
            };
            initialization.push(quote!(let (#slot,#systasis_error_ident)=match #systasis_error_ident {
                error @ ::core::option::Option::Some(_)=>{#skipped_capture (::core::option::Option::None,error)},
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
        let stored_child_values = if !nominal_children {
            quote!(#children_ident)
        } else {
            quote!(__systasis_injected::__StoredChildren { inner: #children_ident, marker: ::core::marker::PhantomData })
        };
        let generated: Block = syn::parse2(quote!({
            #macro_path!(@__systasis_marker);
            #(#policy_checks)*
            #(#validation_calls)*
            #(#capture_initialization)*
            let #children_ident = (#(#child_values,)*);
            let #systasis_error_ident: ::core::option::Option<#error_ty>=::core::result::Result::<(), ::core::convert::Infallible>::Ok(()).map_err(|never| match never {}).err();
            #(#initialization)*
            let #systasis_result_ident=match #systasis_error_ident {
                ::core::option::Option::None=>{
                    let container: #construction_type = AppContainer {#(#values,)*_children: #stored_child_values, _pin: ::core::marker::PhantomPinned,_parameters: ::core::marker::PhantomData};
                    ::core::result::Result::Ok(container)
                },
                #[allow(unreachable_code, reason = "this generated error arm cannot execute for an uninhabited build error type")]
                ::core::option::Option::Some(error)=>{#(::systasis::__private::discard(#reverse);)* ::core::result::Result::Err(error)},
            };
            let (#systasis_value_ident,#systasis_error_ident)=::systasis::__private::split(#systasis_result_ident);
            let #systasis_owner_ident=::core::pin::pin!(#systasis_value_ident);
            let #systasis_published_ident=match (#systasis_owner_ident.as_ref().get_ref(),#systasis_error_ident) {
                (::core::option::Option::Some(container),::core::option::Option::None)=>::core::result::Result::Ok(container),
                #[allow(unreachable_code, reason = "this generated error arm cannot execute for an uninhabited build error type")]
                (::core::option::Option::None,::core::option::Option::Some(error))=>::core::result::Result::Err(error),
                _=>::core::unreachable!("split result has exactly one occupied branch"),
            };
            #(#checks)*
            let #pattern = #published_expression #otherwise;
        }))?;
        statements.extend(generated.stmts);
    }
    function.block.stmts = statements;
    if !opaque_definitions.is_empty() {
        function
            .attrs
            .push(parse_quote!(#[define_opaque(#(#opaque_definitions),*)]));
    }
    let mut definitions: syn::File = syn::parse2(quote!(#emitted))?;
    for item in &mut definitions.items {
        if let syn::Item::Mod(module) = item
            && module.ident == "__systasis_injected"
        {
            crate::rebase::generated_module(module);
        }
    }
    ::core::result::Result::Ok(quote!(#definitions #function))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn direct_native_child_query_rebuilds_only_its_selected_descriptor() {
        let context = format_ident!("__systasis_context");
        let mut expression: Expr = parse_quote!(&self._children.1);
        NativeContext(&context, 3).visit_expr_mut(&mut expression);
        let expected: Expr = parse_quote!(&__systasis_context._children.1.descriptor());
        assert_eq!(
            quote!(#expression).to_string(),
            quote!(#expected).to_string()
        );
    }

    #[test]
    fn native_transitive_call_rebuilds_the_child_tuple() {
        let context = format_ident!("__systasis_context");
        let mut expression: Expr = parse_quote!(construct(self._children));
        NativeContext(&context, 2).visit_expr_mut(&mut expression);
        let expected: Expr = parse_quote!(construct(&(
            __systasis_context._children.0.descriptor(),
            __systasis_context._children.1.descriptor(),
        )));
        assert_eq!(
            quote!(#expression).to_string(),
            quote!(#expected).to_string()
        );
    }

    #[test]
    fn constructor_context_does_not_add_bounds_on_unused_original_generics() {
        let function = parse_quote! {
            fn example<'unused, T>(_: ::core::marker::PhantomData<fn() -> (*const T, &'unused ())>) {
                let Ok(container) = systasis_container! {
                    register_type_with!(usize as IValue, || 7);
                }.build();
            }
        };
        let file: File = syn::parse2(expand(function, false, &[]).unwrap()).unwrap();
        let module = file
            .items
            .iter()
            .find_map(|item| match item {
                Item::Mod(module) if module.ident == "__systasis_injected" => Some(module),
                _ => None,
            })
            .unwrap();
        let helper = module
            .content
            .as_ref()
            .unwrap()
            .1
            .iter()
            .find_map(|item| match item {
                Item::Fn(function) if function.sig.ident == "construct_0" => Some(function),
                _ => None,
            })
            .unwrap();
        let predicates = &helper
            .sig
            .generics
            .where_clause
            .as_ref()
            .unwrap()
            .predicates;
        assert!(!predicates.iter().any(|predicate| match predicate {
            WherePredicate::Lifetime(predicate) => predicate.lifetime.ident == "unused",
            WherePredicate::Type(predicate) =>
                matches!(&predicate.bounded_ty, Type::Path(path) if path.path.is_ident("T")),
            _ => false,
        }));
    }
}
