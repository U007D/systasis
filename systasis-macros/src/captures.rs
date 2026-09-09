//! Lexical capture storage for repeatable constructors.
//!
//! Capture types come from explicit source annotations, never expression
//! inference. Rust checks whether the rewritten constructor can run through a
//! shared reference to its owned capture tuple.

use std::collections::{BTreeMap, BTreeSet};

use quote::{ToTokens, format_ident, quote};
use syn::{
    Expr, ExprClosure, FnArg, Ident, Item, ItemFn, Pat, Stmt, Type,
    spanned::Spanned,
    visit_mut::{self, VisitMut},
};

#[derive(Clone, Default)]
pub(crate) struct Bindings {
    types: BTreeMap<String, Option<Type>>,
    uncertain: Option<proc_macro2::TokenStream>,
    imports: Vec<syn::ItemUse>,
    generic_names: BTreeSet<String>,
    tuple_projections: BTreeSet<(usize, usize)>,
    sequence_projections: bool,
}

#[derive(Clone, Copy)]
enum BindingMode {
    Move,
    Ref,
    RefMut,
}

impl BindingMode {
    fn dereferenced(self, mutable: bool) -> Self {
        match (self, mutable) {
            (Self::Ref, _) | (_, false) => Self::Ref,
            (_, true) => Self::RefMut,
        }
    }

    fn bound_type(self, annotation: &Type) -> Type {
        match self {
            Self::Move => annotation.clone(),
            Self::Ref => syn::parse_quote!(&'_ #annotation),
            Self::RefMut => syn::parse_quote!(&'_ mut #annotation),
        }
    }
}

impl Bindings {
    pub(crate) fn from_function(function: &ItemFn) -> Self {
        let generic_names = function
            .sig
            .generics
            .params
            .iter()
            .filter_map(|parameter| match parameter {
                syn::GenericParam::Type(parameter) => Some(name(&parameter.ident)),
                syn::GenericParam::Const(parameter) => Some(name(&parameter.ident)),
                syn::GenericParam::Lifetime(_) => None,
            })
            .collect();
        let mut bindings = Self {
            generic_names,
            ..Self::default()
        };
        let local_items = function
            .block
            .stmts
            .iter()
            .filter_map(|statement| {
                let Stmt::Item(item) = statement else {
                    return None;
                };
                match item {
                    Item::Mod(item) => Some(name(&item.ident)),
                    Item::Type(item) => Some(name(&item.ident)),
                    Item::Struct(item) => Some(name(&item.ident)),
                    Item::Enum(item) => Some(name(&item.ident)),
                    Item::Union(item) => Some(name(&item.ident)),
                    Item::ExternCrate(item) => Some(name(
                        item.rename.as_ref().map_or(&item.ident, |(_, name)| name),
                    )),
                    _ => None,
                }
            })
            .collect::<BTreeSet<_>>();
        for argument in &function.sig.inputs {
            if let FnArg::Typed(argument) = argument {
                bindings.observe_pattern(&argument.pat, Some(&argument.ty));
            }
        }
        // Items have block-wide scope; subsequent local bindings can still
        // shadow their names as observe_statement walks toward the container.
        for statement in &function.block.stmts {
            if let Stmt::Item(item) = statement {
                if let Some(ident) = item_name(item) {
                    bindings.types.remove(&name(ident));
                }
                if let Item::Use(import) = item {
                    let mut names = BTreeSet::new();
                    let anchored = import.leading_colon.is_some()
                        || matches!(&import.tree, syn::UseTree::Path(path) if path.ident == "crate" || path.ident == "self" || path.ident == "super");
                    let independent = anchored
                        || import_roots(&import.tree)
                            .iter()
                            .all(|root| !local_items.contains(root));
                    if independent
                        && import_names(&import.tree, &mut names)
                        && names.iter().all(|name| !bindings.types.contains_key(name))
                    {
                        bindings.imports.push(import.clone());
                    } else {
                        bindings.uncertain = Some(item.to_token_stream());
                    }
                }
                if matches!(item, Item::Macro(_)) {
                    bindings.uncertain = Some(item.to_token_stream());
                }
            }
        }
        bindings
    }

    pub(crate) fn imports(&self) -> &[syn::ItemUse] {
        &self.imports
    }

    /// Emit only structural implementations: no source alias or concrete type
    /// appears here. Each authored exact tuple shape supplies its own arity.
    pub(crate) fn projection_helpers(&self) -> proc_macro2::TokenStream {
        if self.tuple_projections.is_empty() && !self.sequence_projections {
            return quote!();
        }
        let implementations = self.tuple_projections.iter().map(|&(arity, index)| {
            let projection = format_ident!("__SystasisCaptureTuple{arity}_{index}");
            let elements = (0..arity).map(|i| format_ident!("__Element{i}")).collect::<Vec<_>>();
            let selected = &elements[index];
            quote! {
                pub trait #projection<__Mode = __CaptureOwned> { type Output; }
                impl<#(#elements,)* __Mode: __CaptureWrap<#selected>> #projection<__Mode> for (#(#elements,)*) {
                    type Output = <__Mode as __CaptureWrap<#selected>>::Output;
                }
                impl<'a, __Target: ?Sized, __Mode> #projection<__Mode> for &'a __Target
                where __Target: #projection<__CaptureShared<'a>> {
                    type Output = <__Target as #projection<__CaptureShared<'a>>>::Output;
                }
                impl<'a, __Target: ?Sized, __Mode: __CaptureThroughMut<'a>> #projection<__Mode> for &'a mut __Target
                where __Target: #projection<<__Mode as __CaptureThroughMut<'a>>::Mode> {
                    type Output = <__Target as #projection<<__Mode as __CaptureThroughMut<'a>>::Mode>>::Output;
                }
            }
        });
        let sequence = self.sequence_projections.then(|| quote! {
            pub trait __SystasisCaptureElement<Mode = __CaptureOwned> { type Output; }
            impl<T, const N: usize, Mode: __CaptureWrap<T>> __SystasisCaptureElement<Mode> for [T; N] {
                type Output = <Mode as __CaptureWrap<T>>::Output;
            }
            impl<T, Mode: __CaptureWrap<T>> __SystasisCaptureElement<Mode> for [T] {
                type Output = <Mode as __CaptureWrap<T>>::Output;
            }
            impl<'a, T: ?Sized, Mode> __SystasisCaptureElement<Mode> for &'a T
            where T: __SystasisCaptureElement<__CaptureShared<'a>> {
                type Output = <T as __SystasisCaptureElement<__CaptureShared<'a>>>::Output;
            }
            impl<'a, T: ?Sized, Mode: __CaptureThroughMut<'a>> __SystasisCaptureElement<Mode> for &'a mut T
            where T: __SystasisCaptureElement<<Mode as __CaptureThroughMut<'a>>::Mode> {
                type Output = <T as __SystasisCaptureElement<<Mode as __CaptureThroughMut<'a>>::Mode>>::Output;
            }
            pub trait __SystasisCaptureLength<const K: usize> { const REMAINING: usize; }
            impl<T, const N: usize, const K: usize> __SystasisCaptureLength<K> for [T; N] { const REMAINING: usize = N - K; }
            // Slice tail projection ignores the const argument; its type is [T].
            impl<T, const K: usize> __SystasisCaptureLength<K> for [T] { const REMAINING: usize = 0; }
            impl<T: ?Sized + __SystasisCaptureLength<K>, const K: usize> __SystasisCaptureLength<K> for &T { const REMAINING: usize = T::REMAINING; }
            impl<T: ?Sized + __SystasisCaptureLength<K>, const K: usize> __SystasisCaptureLength<K> for &mut T { const REMAINING: usize = T::REMAINING; }
            pub trait __SystasisCaptureArrayTail<const R: usize, Mode = __CaptureOwned> { type Output; }
            impl<T, const N: usize, const R: usize, Mode: __CaptureWrap<[T; R]>> __SystasisCaptureArrayTail<R, Mode> for [T; N] {
                type Output = <Mode as __CaptureWrap<[T; R]>>::Output;
            }
            impl<T, const R: usize, Mode: __CaptureSliceWrap<T>> __SystasisCaptureArrayTail<R, Mode> for [T] {
                type Output = <Mode as __CaptureSliceWrap<T>>::Output;
            }
            impl<'a, T: ?Sized, const R: usize, Mode> __SystasisCaptureArrayTail<R, Mode> for &'a T
            where T: __SystasisCaptureArrayTail<R, __CaptureShared<'a>> {
                type Output = <T as __SystasisCaptureArrayTail<R, __CaptureShared<'a>>>::Output;
            }
            impl<'a, T: ?Sized, const R: usize, Mode: __CaptureThroughMut<'a>> __SystasisCaptureArrayTail<R, Mode> for &'a mut T
            where T: __SystasisCaptureArrayTail<R, <Mode as __CaptureThroughMut<'a>>::Mode> {
                type Output = <T as __SystasisCaptureArrayTail<R, <Mode as __CaptureThroughMut<'a>>::Mode>>::Output;
            }
            pub trait __CaptureSliceWrap<T> { type Output; }
            impl<'a, T: 'a> __CaptureSliceWrap<T> for __CaptureShared<'a> { type Output = &'a [T]; }
            impl<'a, T: 'a> __CaptureSliceWrap<T> for __CaptureMutable<'a> { type Output = &'a mut [T]; }
            pub trait __SystasisCaptureSliceTail<Mode = __CaptureOwned> { type Output; }
            impl<T, Mode: __CaptureSliceWrap<T>> __SystasisCaptureSliceTail<Mode> for [T] {
                type Output = <Mode as __CaptureSliceWrap<T>>::Output;
            }
            impl<'a, T: ?Sized, Mode> __SystasisCaptureSliceTail<Mode> for &'a T
            where T: __SystasisCaptureSliceTail<__CaptureShared<'a>> {
                type Output = <T as __SystasisCaptureSliceTail<__CaptureShared<'a>>>::Output;
            }
            impl<'a, T: ?Sized, Mode: __CaptureThroughMut<'a>> __SystasisCaptureSliceTail<Mode> for &'a mut T
            where T: __SystasisCaptureSliceTail<<Mode as __CaptureThroughMut<'a>>::Mode> {
                type Output = <T as __SystasisCaptureSliceTail<<Mode as __CaptureThroughMut<'a>>::Mode>>::Output;
            }
        });
        quote! {
            pub struct __CaptureOwned;
            pub struct __CaptureShared<'a>(::core::marker::PhantomData<&'a ()>);
            pub struct __CaptureMutable<'a>(::core::marker::PhantomData<&'a ()>);
            pub trait __CaptureWrap<T> { type Output; }
            impl<T> __CaptureWrap<T> for __CaptureOwned { type Output = T; }
            impl<'a, T: 'a> __CaptureWrap<T> for __CaptureShared<'a> { type Output = &'a T; }
            impl<'a, T: 'a> __CaptureWrap<T> for __CaptureMutable<'a> { type Output = &'a mut T; }
            pub trait __CaptureThroughMut<'a> { type Mode; }
            impl<'a> __CaptureThroughMut<'a> for __CaptureOwned { type Mode = __CaptureMutable<'a>; }
            impl<'a, 'b> __CaptureThroughMut<'b> for __CaptureShared<'a> { type Mode = __CaptureShared<'a>; }
            impl<'a, 'b> __CaptureThroughMut<'b> for __CaptureMutable<'a> { type Mode = __CaptureMutable<'a>; }
            #(#implementations)*
            #sequence
        }
    }

    /// Call in source order, only for statements preceding the container.
    pub(crate) fn observe_statement(&mut self, statement: &Stmt) {
        match statement {
            Stmt::Local(local) => self.observe_pattern(&local.pat, None),
            Stmt::Macro(_) => self.uncertain = Some(statement.to_token_stream()),
            _ => {}
        }
    }

    fn observe_pattern(&mut self, pattern: &Pat, annotation: Option<&Type>) {
        if let Pat::Type(typed) = pattern {
            self.observe_pattern(&typed.pat, Some(&typed.ty));
            return;
        }
        let mut names = BTreeSet::new();
        if pattern_names(pattern, &mut names) {
            self.uncertain = Some(pattern.to_token_stream());
        }
        for name in names {
            self.types.insert(name, None);
        }
        if let Some(annotation) = annotation {
            self.observe_annotated_pattern(pattern, annotation, BindingMode::Move);
        }
    }

    /// Extract structurally annotated types and propagate Rust's binding modes.
    /// Structural projections cover selected alias shapes; struct fields still
    /// need unavailable type information.
    /// https://doc.rust-lang.org/reference/patterns.html#binding-modes
    fn observe_annotated_pattern(&mut self, pattern: &Pat, annotation: &Type, mode: BindingMode) {
        match (pattern, annotation) {
            (_, Type::Paren(ty)) => self.observe_annotated_pattern(pattern, &ty.elem, mode),
            (_, Type::Group(ty)) => self.observe_annotated_pattern(pattern, &ty.elem, mode),
            (Pat::Paren(pattern), _) => {
                self.observe_annotated_pattern(&pattern.pat, annotation, mode);
            }
            (Pat::Ident(binding), _) if binding.subpat.is_none() => {
                // Keep explicit modifiers' pre-2024 meaning too. The original
                // pattern remains in the function, so rustc enforces edition
                // restrictions rather than the macro silently accepting it.
                let mode = match (binding.by_ref, binding.mutability) {
                    (Some(_), Some(_)) => BindingMode::RefMut,
                    (Some(_), None) => BindingMode::Ref,
                    (None, Some(_)) => BindingMode::Move,
                    (None, None) => mode,
                };
                self.types
                    .insert(name(&binding.ident), Some(mode.bound_type(annotation)));
            }
            (Pat::Reference(pattern), Type::Reference(ty)) => {
                self.observe_annotated_pattern(&pattern.pat, &ty.elem, BindingMode::Move);
            }
            (Pat::Tuple(_) | Pat::Slice(_), Type::Reference(ty)) => {
                self.observe_annotated_pattern(
                    pattern,
                    &ty.elem,
                    mode.dereferenced(ty.mutability.is_some()),
                );
            }
            (Pat::Tuple(pattern), Type::Tuple(ty)) => {
                let rest = pattern
                    .elems
                    .iter()
                    .position(|pat| matches!(pat, Pat::Rest(_)));
                let explicit = pattern.elems.len() - usize::from(rest.is_some());
                if explicit > ty.elems.len() || (rest.is_none() && explicit != ty.elems.len()) {
                    return;
                }
                for (index, element) in pattern.elems.iter().enumerate() {
                    if matches!(element, Pat::Rest(_)) {
                        continue;
                    }
                    let type_index = if rest.is_some_and(|rest| index > rest) {
                        ty.elems.len() - (pattern.elems.len() - index)
                    } else {
                        index
                    };
                    self.observe_annotated_pattern(element, &ty.elems[type_index], mode);
                }
            }
            (Pat::Slice(pattern), Type::Array(ty)) => {
                let explicit = pattern.elems.iter().filter(|pat| !slice_rest(pat)).count();
                let rests = pattern.elems.len() - explicit;
                let length = match &ty.len {
                    Expr::Lit(literal) => match &literal.lit {
                        syn::Lit::Int(integer) => integer.base10_parse::<usize>().ok(),
                        _ => None,
                    },
                    _ => None,
                };
                if rests > 1
                    || length.is_some_and(|length| {
                        explicit > length || (rests == 0 && explicit != length)
                    })
                {
                    return;
                }
                let rest_type = if explicit == 0 {
                    // Preserve the authored length; rustc still validates the
                    // original pattern (including generic-length restrictions).
                    Some(annotation.clone())
                } else {
                    length
                        .map(|length| {
                            let remaining =
                                syn::LitInt::new(&(length - explicit).to_string(), ty.len.span());
                            let element = &ty.elem;
                            syn::parse_quote!([#element; #remaining])
                        })
                        .or_else(|| {
                            // Preserve concrete const expressions for rustc to
                            // evaluate; never introduce subtraction on generics.
                            concrete_length(&ty.len, &self.generic_names).then(|| {
                                let length = &ty.len;
                                let element = &ty.elem;
                                let explicit =
                                    syn::LitInt::new(&explicit.to_string(), length.span());
                                syn::parse_quote!([#element; (#length) - #explicit])
                            })
                        })
                };
                for element in &pattern.elems {
                    self.observe_slice_element(element, &ty.elem, rest_type.as_ref(), mode);
                }
            }
            (Pat::Slice(pattern), Type::Slice(ty)) => {
                if pattern.elems.iter().filter(|pat| slice_rest(pat)).count() > 1 {
                    return;
                }
                for element in &pattern.elems {
                    self.observe_slice_element(element, &ty.elem, Some(annotation), mode);
                }
            }
            (Pat::Slice(pattern), Type::Path(path)) => {
                self.sequence_projections = true;
                let source = mode.bound_type(annotation);
                let element = syn::parse_quote!(<#source as __systasis_injected::__SystasisCaptureElement>::Output);
                let explicit = pattern.elems.iter().filter(|pat| !slice_rest(pat)).count();
                let concrete = path.qself.is_none()
                    && path.path.segments.iter().all(|segment| {
                        !self.generic_names.contains(&name(&segment.ident))
                            && matches!(segment.arguments, syn::PathArguments::None)
                    });
                // Concrete aliases permit an associated-const tail length;
                // borrow mode changes only the output wrapper, not that length.
                // Generic slice aliases use exact slice projections;
                // generic array remainders have no implementation here.
                let rest = if concrete {
                    syn::parse_quote!(<#source as __systasis_injected::__SystasisCaptureArrayTail<{<#annotation as __systasis_injected::__SystasisCaptureLength<#explicit>>::REMAINING}>>::Output)
                } else {
                    syn::parse_quote!(<#source as __systasis_injected::__SystasisCaptureSliceTail>::Output)
                };
                for pat in &pattern.elems {
                    self.observe_slice_element(pat, &element, Some(&rest), BindingMode::Move);
                }
            }
            (Pat::Tuple(pattern), Type::Path(_))
                if !pattern
                    .elems
                    .iter()
                    .any(|element| matches!(element, Pat::Rest(_))) =>
            {
                let arity = pattern.elems.len();
                let source = mode.bound_type(annotation);
                for (index, element) in pattern.elems.iter().enumerate() {
                    if matches!(element, Pat::Wild(_)) {
                        continue;
                    }
                    self.tuple_projections.insert((arity, index));
                    let projection = format_ident!("__SystasisCaptureTuple{arity}_{index}");
                    let ty =
                        syn::parse_quote!(<#source as __systasis_injected::#projection>::Output);
                    self.observe_annotated_pattern(element, &ty, BindingMode::Move);
                }
            }
            (Pat::Reference(pattern), Type::Path(_)) => {
                let ty = syn::parse_quote!(<#annotation as ::core::ops::Deref>::Target);
                self.observe_annotated_pattern(&pattern.pat, &ty, BindingMode::Move);
            }
            _ => {}
        }
    }

    fn observe_slice_element(
        &mut self,
        pattern: &Pat,
        element: &Type,
        rest: Option<&Type>,
        mode: BindingMode,
    ) {
        if slice_rest(pattern) {
            if let (Pat::Ident(binding), Some(rest)) = (pattern, rest) {
                let mut binding = binding.clone();
                binding.subpat = None;
                self.observe_annotated_pattern(&Pat::Ident(binding), rest, mode);
            }
        } else {
            self.observe_annotated_pattern(pattern, element, mode);
        }
    }
}

fn slice_rest(pattern: &Pat) -> bool {
    matches!(pattern, Pat::Rest(_))
        || matches!(pattern, Pat::Ident(binding) if binding.subpat.as_ref().is_some_and(|(_, subpat)| matches!(subpat.as_ref(), Pat::Rest(_))))
}

fn concrete_length(expression: &Expr, generics: &BTreeSet<String>) -> bool {
    match expression {
        Expr::Lit(_) => true,
        Expr::Path(path) => {
            path.qself.is_none()
                && path.path.segments.iter().all(|segment| {
                    segment.ident != "Self"
                        && !generics.contains(&name(&segment.ident))
                        && matches!(segment.arguments, syn::PathArguments::None)
                })
        }
        Expr::Binary(binary) => {
            concrete_length(&binary.left, generics) && concrete_length(&binary.right, generics)
        }
        Expr::Unary(unary) => concrete_length(&unary.expr, generics),
        Expr::Paren(paren) => concrete_length(&paren.expr, generics),
        Expr::Group(group) => concrete_length(&group.expr, generics),
        _ => false,
    }
}

pub(crate) struct Plan {
    pub(crate) captures: Vec<(Ident, Type)>,
    pub(crate) closure: ExprClosure,
    pub(crate) requires_record: bool,
}

#[derive(Default)]
struct CaptureProjection(bool);
impl VisitMut for CaptureProjection {
    fn visit_type_path_mut(&mut self, path: &mut syn::TypePath) {
        self.0 |= path.qself.is_some()
            && path
                .path
                .segments
                .first()
                .is_some_and(|segment| segment.ident == "__systasis_injected")
            && path
                .path
                .segments
                .iter()
                .any(|segment| segment.ident.to_string().starts_with("__SystasisCapture"));
        visit_mut::visit_type_path_mut(self, path);
    }
}

pub(crate) fn prepare(
    closure: &ExprClosure,
    bindings: &Bindings,
    capture_root: &Ident,
) -> syn::Result<Plan> {
    if let Some(tokens) = &bindings.uncertain {
        return Err(syn::Error::new_spanned(
            tokens,
            "constructor capture analysis cannot inspect bindings introduced by opaque macros or function-local imports",
        ));
    }
    let mut closure = closure.clone();
    let mut visitor = Visitor {
        bindings,
        capture_root,
        scopes: vec![BTreeSet::new()],
        glob_scopes: vec![None],
        captures: Vec::new(),
        error: None,
    };
    visitor.visit_expr_closure_mut(&mut closure);
    if let Some(error) = visitor.error {
        return Err(error);
    }
    let mut projection = CaptureProjection::default();
    for (_, ty) in &visitor.captures {
        projection.visit_type_mut(&mut ty.clone());
    }
    Ok(Plan {
        captures: visitor.captures,
        closure,
        requires_record: projection.0,
    })
}

fn name(ident: &Ident) -> String {
    ident.to_string().trim_start_matches("r#").to_owned()
}

// Explicit import names can be preserved without resolving a glob's members.
fn import_names(tree: &syn::UseTree, output: &mut BTreeSet<String>) -> bool {
    match tree {
        syn::UseTree::Path(path) => import_names(&path.tree, output),
        syn::UseTree::Name(binding) if binding.ident != "self" => {
            output.insert(name(&binding.ident));
            true
        }
        syn::UseTree::Name(_) => false,
        syn::UseTree::Rename(binding) => {
            output.insert(name(&binding.rename));
            true
        }
        syn::UseTree::Group(group) => group.items.iter().all(|item| import_names(item, output)),
        syn::UseTree::Glob(_) => false,
    }
}

fn import_roots(tree: &syn::UseTree) -> Vec<String> {
    match tree {
        syn::UseTree::Path(path) => vec![name(&path.ident)],
        syn::UseTree::Name(binding) => vec![name(&binding.ident)],
        syn::UseTree::Rename(binding) => vec![name(&binding.ident)],
        syn::UseTree::Group(group) => group.items.iter().flat_map(import_roots).collect(),
        syn::UseTree::Glob(_) => Vec::new(),
    }
}

fn contains_glob(tree: &syn::UseTree) -> bool {
    match tree {
        syn::UseTree::Glob(_) => true,
        syn::UseTree::Path(path) => contains_glob(&path.tree),
        syn::UseTree::Group(group) => group.items.iter().any(contains_glob),
        _ => false,
    }
}

/// Return true for opaque patterns whose introduced names are unknown.
fn pattern_names(pattern: &Pat, output: &mut BTreeSet<String>) -> bool {
    match pattern {
        Pat::Ident(pattern) => {
            output.insert(name(&pattern.ident));
            pattern
                .subpat
                .as_ref()
                .is_some_and(|(_, pattern)| pattern_names(pattern, output))
        }
        Pat::Tuple(pattern) => pattern.elems.iter().fold(false, |opaque, pattern| {
            pattern_names(pattern, output) | opaque
        }),
        Pat::TupleStruct(pattern) => pattern.elems.iter().fold(false, |opaque, pattern| {
            pattern_names(pattern, output) | opaque
        }),
        Pat::Struct(pattern) => pattern.fields.iter().fold(false, |opaque, field| {
            pattern_names(&field.pat, output) | opaque
        }),
        Pat::Slice(pattern) => pattern.elems.iter().fold(false, |opaque, pattern| {
            pattern_names(pattern, output) | opaque
        }),
        Pat::Or(pattern) => pattern.cases.iter().fold(false, |opaque, pattern| {
            pattern_names(pattern, output) | opaque
        }),
        Pat::Reference(pattern) => pattern_names(&pattern.pat, output),
        Pat::Guard(pattern) => pattern_names(&pattern.pat, output),
        Pat::Type(pattern) => pattern_names(&pattern.pat, output),
        Pat::Paren(pattern) => pattern_names(&pattern.pat, output),
        Pat::Macro(_) | Pat::Verbatim(_) => true,
        _ => false,
    }
}

fn item_name(item: &Item) -> Option<&Ident> {
    match item {
        Item::Fn(item) => Some(&item.sig.ident),
        Item::Const(item) => Some(&item.ident),
        Item::Static(item) => Some(&item.ident),
        Item::Struct(item) if !matches!(item.fields, syn::Fields::Named(_)) => Some(&item.ident),
        _ => None,
    }
}

struct Visitor<'a> {
    bindings: &'a Bindings,
    capture_root: &'a Ident,
    scopes: Vec<BTreeSet<String>>,
    glob_scopes: Vec<Option<proc_macro2::TokenStream>>,
    captures: Vec<(Ident, Type)>,
    error: Option<syn::Error>,
}

/// Syn 3 stores match guards inside patterns rather than on match arms.
struct PatternExpressions<'a, 'b>(&'a mut Visitor<'b>);

impl VisitMut for PatternExpressions<'_, '_> {
    fn visit_expr_mut(&mut self, expression: &mut Expr) {
        self.0.visit_expr_mut(expression);
    }

    fn visit_type_mut(&mut self, _: &mut Type) {}
}

impl Visitor<'_> {
    fn reject(&mut self, tokens: impl ToTokens, message: &str) {
        let error = syn::Error::new_spanned(tokens, message);
        if let Some(existing) = &mut self.error {
            existing.combine(error);
        } else {
            self.error = Some(error);
        }
    }

    fn bind(&mut self, pattern: &Pat) {
        let scope = self.scopes.last_mut().unwrap_or_else(|| unreachable!(
            "prepare installs a root scope; nested traversals push before popping; bind does not remove scopes"
        ));
        if pattern_names(pattern, scope) {
            self.reject(
                pattern,
                "constructor capture analysis cannot inspect this pattern macro",
            );
        }
    }

    fn capture_index(&mut self, ident: &Ident) -> Option<syn::Index> {
        let key = name(ident);
        if self.scopes.iter().any(|scope| scope.contains(&key)) {
            return None;
        }
        let annotation = self.bindings.types.get(&key)?;
        if let Some(import) = self.glob_scopes.iter().flatten().last() {
            self.reject(import.clone(), "constructor capture analysis cannot determine whether this glob import shadows the referenced outer binding");
            return None;
        }
        let Some(ty) = annotation else {
            self.reject(ident, "captured constructor bindings require an explicit type annotation from which the binding's type can be determined");
            return None;
        };
        let index = self
            .captures
            .iter()
            .position(|(binding, _)| name(binding) == key)
            .unwrap_or_else(|| {
                self.captures.push((ident.clone(), ty.clone()));
                self.captures.len() - 1
            });
        Some(syn::Index::from(index))
    }
}

impl VisitMut for Visitor<'_> {
    // Items cannot capture their enclosing function's locals. Their contents
    // are deliberately not rewritten, including any macros inside those items.
    fn visit_item_mut(&mut self, _: &mut Item) {}
    fn visit_type_mut(&mut self, _: &mut Type) {}
    fn visit_pat_mut(&mut self, _: &mut Pat) {}

    fn visit_expr_closure_mut(&mut self, closure: &mut ExprClosure) {
        self.scopes.push(BTreeSet::new());
        self.glob_scopes.push(None);
        for input in &closure.inputs {
            self.bind(input);
        }
        self.visit_expr_mut(&mut closure.body);
        self.scopes.pop();
        self.glob_scopes.pop();
    }

    fn visit_block_mut(&mut self, block: &mut syn::Block) {
        let mut scope = BTreeSet::new();
        let mut glob = None;
        // Block items are in scope throughout the block, before their textual
        // declaration. Imports need Rust name resolution to identify namespaces.
        for statement in &block.stmts {
            if let Stmt::Item(item) = statement {
                if let Some(ident) = item_name(item) {
                    scope.insert(name(ident));
                }
                if let Item::Use(import) = item {
                    let mut imported = BTreeSet::new();
                    // Block-local items move with the constructor body, unlike
                    // items in the enclosing configuration function.
                    if contains_glob(&import.tree) {
                        glob = Some(import.to_token_stream());
                    } else if !import_names(&import.tree, &mut imported)
                        || imported
                            .iter()
                            .any(|name| self.bindings.types.contains_key(name))
                    {
                        self.reject(
                            item,
                            "constructor capture analysis cannot resolve this block-local import",
                        );
                    } else {
                        scope.extend(imported);
                    }
                }
                if matches!(item, Item::Macro(_)) {
                    self.reject(item, "constructor capture analysis cannot inspect block-local imports or item macros");
                }
            }
        }
        self.scopes.push(scope);
        self.glob_scopes.push(glob);
        for statement in &mut block.stmts {
            match statement {
                Stmt::Local(local) => {
                    if let Some(initializer) = &mut local.init {
                        self.visit_expr_mut(&mut initializer.expr);
                        if let Some((_, diverge)) = &mut initializer.diverge {
                            self.visit_expr_mut(diverge);
                        }
                    }
                    self.bind(&local.pat);
                }
                Stmt::Macro(invocation) => self.reject(
                    invocation,
                    "constructor capture analysis cannot inspect opaque macros",
                ),
                Stmt::Expr(expression, _) => self.visit_expr_mut(expression),
                Stmt::Item(_) => {}
            }
        }
        self.scopes.pop();
        self.glob_scopes.pop();
    }

    fn visit_field_value_mut(&mut self, field: &mut syn::FieldValue) {
        let before = field.expr.to_token_stream().to_string();
        self.visit_expr_mut(&mut field.expr);
        if field.colon_token.is_none() && before != field.expr.to_token_stream().to_string() {
            field.colon_token = Some(Default::default());
        }
    }

    fn visit_expr_mut(&mut self, expression: &mut Expr) {
        if let Expr::Path(path) = expression
            && path.qself.is_none()
            && path.path.leading_colon.is_none()
            && path.path.segments.len() == 1
            && let Some(segment) = path.path.segments.first()
            && let Some(index) = self.capture_index(&segment.ident)
        {
            let root = self.capture_root;
            let attrs = &path.attrs;
            *expression = syn::parse_quote!(#(#attrs)* #root.#index);
            return;
        }
        match expression {
            Expr::Macro(invocation) => self.reject(
                invocation,
                "constructor capture analysis cannot inspect opaque macros",
            ),
            Expr::Verbatim(tokens) => self.reject(
                tokens,
                "constructor capture analysis cannot inspect this syntax",
            ),
            Expr::ForLoop(loop_) => {
                self.visit_expr_mut(&mut loop_.expr);
                self.scopes.push(BTreeSet::new());
                self.bind(&loop_.pat);
                self.visit_block_mut(&mut loop_.body);
                self.scopes.pop();
            }
            Expr::If(if_) => {
                self.scopes.push(BTreeSet::new());
                self.visit_expr_mut(&mut if_.cond);
                self.visit_block_mut(&mut if_.then_branch);
                self.scopes.pop();
                if let Some((_, else_)) = &mut if_.else_branch {
                    self.visit_expr_mut(else_);
                }
            }
            Expr::While(while_) => {
                self.scopes.push(BTreeSet::new());
                self.visit_expr_mut(&mut while_.cond);
                self.visit_block_mut(&mut while_.body);
                self.scopes.pop();
            }
            Expr::Let(let_) => {
                self.visit_expr_mut(&mut let_.expr);
                self.bind(&let_.pat);
            }
            Expr::Match(match_) => {
                self.visit_expr_mut(&mut match_.expr);
                for arm in &mut match_.arms {
                    self.scopes.push(BTreeSet::new());
                    self.bind(&arm.pat);
                    PatternExpressions(self).visit_pat_mut(&mut arm.pat);
                    self.visit_expr_mut(&mut arm.body);
                    self.scopes.pop();
                }
            }
            _ => visit_mut::visit_expr_mut(self, expression),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use syn::parse_quote;

    fn plan(closure: ExprClosure) -> Plan {
        let function = parse_quote!(
            fn main(config: Config, url: String) {}
        );
        prepare(
            &closure,
            &Bindings::from_function(&function),
            &parse_quote!(__captures),
        )
        .unwrap_or_else(|error| panic!("unexpected capture error: {error}"))
    }

    #[test]
    fn methods_fields_and_shorthand_use_owned_tuple_places() {
        let result = plan(parse_quote!(move || {
            consume(config.port);
            Output { url }
        }));
        assert_eq!(result.captures.len(), 2);
        let expected: ExprClosure = parse_quote!(move || {
            consume(__captures.0.port);
            Output { url: __captures.1 }
        });
        assert_eq!(
            result.closure.to_token_stream().to_string(),
            expected.to_token_stream().to_string()
        );
    }

    #[test]
    fn locals_and_nested_closure_parameters_shadow_captures() {
        let result = plan(parse_quote!(|| {
            let url = url.clone();
            let nested = |config: Config| config.port;
            (url, nested(config.clone()))
        }));
        assert_eq!(
            result
                .captures
                .iter()
                .map(|(ident, _)| name(ident))
                .collect::<Vec<_>>(),
            ["url", "config"]
        );
        let expected: ExprClosure = parse_quote!(|| {
            let url = __captures.0.clone();
            let nested = |config: Config| config.port;
            (url, nested(__captures.1.clone()))
        });
        assert_eq!(
            result.closure.to_token_stream().to_string(),
            expected.to_token_stream().to_string()
        );
    }

    #[test]
    fn untyped_shadow_replaces_earlier_annotation() {
        let function = parse_quote!(
            fn main(url: String) {}
        );
        let mut bindings = Bindings::from_function(&function);
        bindings.observe_statement(&parse_quote!(let url = String::new();));
        let error = prepare(
            &parse_quote!(|| url.clone()),
            &bindings,
            &parse_quote!(__captures),
        )
        .err()
        .expect("untyped capture must fail");
        assert!(error.to_string().contains("explicit type annotation"));
    }

    #[test]
    fn typed_local_capture_is_recorded() {
        let mut bindings = Bindings::default();
        bindings.observe_statement(&parse_quote!(let mut url: String = String::new();));
        let result = prepare(
            &parse_quote!(|| url.clone()),
            &bindings,
            &parse_quote!(__captures),
        )
        .unwrap();
        assert_eq!(result.captures[0].1.to_token_stream().to_string(), "String");
    }

    #[test]
    fn absolute_imports_are_preserved_without_guessing_namespace_collisions() {
        let function = parse_quote!(
            fn main(config: String) {
                use crate::settings::Config;
            }
        );
        let bindings = Bindings::from_function(&function);
        assert_eq!(bindings.imports().len(), 1);
        assert!(
            prepare(
                &parse_quote!(|| config.clone()),
                &bindings,
                &parse_quote!(__captures)
            )
            .is_ok()
        );

        let function = parse_quote!(
            fn main(Config: String) {
                use crate::settings::Config;
            }
        );
        assert!(
            prepare(
                &parse_quote!(|| Config),
                &Bindings::from_function(&function),
                &parse_quote!(__captures)
            )
            .is_err()
        );
    }

    #[test]
    fn explicit_external_imports_are_preserved_but_local_module_imports_are_not() {
        let function = parse_quote!(
            fn main(config: String) {
                use std::string::String as Text;
                use systasis::systasis_container;
            }
        );
        let bindings = Bindings::from_function(&function);
        assert_eq!(bindings.imports().len(), 2);
        assert!(
            prepare(
                &parse_quote!(|| config.clone()),
                &bindings,
                &parse_quote!(__captures)
            )
            .is_ok()
        );

        let function = parse_quote!(
            fn main(config: String) {
                use std::Text;
                mod std {
                    pub struct Text;
                }
            }
        );
        let bindings = Bindings::from_function(&function);
        assert!(bindings.imports().is_empty());
        assert!(
            prepare(
                &parse_quote!(|| config.clone()),
                &bindings,
                &parse_quote!(__captures)
            )
            .is_err()
        );
    }

    #[test]
    fn match_arm_bindings_do_not_escape() {
        let result = plan(parse_quote!(|| {
            match something() {
                Some(url) if url.is_empty() => url,
                _ => url.clone(),
            }
        }));
        assert_eq!(result.captures.len(), 1);
        let expected: ExprClosure = parse_quote!(|| {
            match something() {
                Some(url) if url.is_empty() => url,
                _ => __captures.0.clone(),
            }
        });
        assert_eq!(
            result.closure.to_token_stream().to_string(),
            expected.to_token_stream().to_string()
        );
    }

    #[test]
    fn opaque_macro_capture_is_rejected() {
        let bindings = Bindings::from_function(&parse_quote!(
            fn main(url: String) {}
        ));
        let error = prepare(
            &parse_quote!(|| format!("{url}")),
            &bindings,
            &parse_quote!(__captures),
        )
        .err()
        .expect("opaque macro must fail");
        assert!(error.to_string().contains("opaque macros"));
    }

    #[test]
    fn match_guard_captures_outer_values_but_not_pattern_bindings() {
        let result = plan(parse_quote!(|| {
            match something() {
                Some(url) if url == config.name => url,
                _ => url.clone(),
            }
        }));
        let expected: ExprClosure = parse_quote!(|| {
            match something() {
                Some(url) if url == __captures.0.name => url,
                _ => __captures.1.clone(),
            }
        });
        assert_eq!(
            result.closure.to_token_stream().to_string(),
            expected.to_token_stream().to_string()
        );
    }

    #[test]
    fn if_let_shadow_does_not_extend_to_else() {
        let result = plan(parse_quote!(|| {
            if let Some(url) = something() {
                url
            } else {
                url.clone()
            }
        }));
        let expected: ExprClosure = parse_quote!(|| {
            if let Some(url) = something() {
                url
            } else {
                __captures.0.clone()
            }
        });
        assert_eq!(
            result.closure.to_token_stream().to_string(),
            expected.to_token_stream().to_string()
        );
    }

    #[test]
    fn for_pattern_and_nested_move_closure_preserve_shadowing() {
        let result = plan(parse_quote!(move || {
            for url in values() {
                consume(url);
            }
            move |config: Config| (config, url.clone())
        }));
        let expected: ExprClosure = parse_quote!(move || {
            for url in values() {
                consume(url);
            }
            move |config: Config| (config, __captures.0.clone())
        });
        assert_eq!(
            result.closure.to_token_stream().to_string(),
            expected.to_token_stream().to_string()
        );
    }

    #[test]
    fn item_constructor_shadows_outer_binding_throughout_block() {
        let result = plan(parse_quote!(|| {
            let value = url;
            struct url;
            value
        }));
        assert!(result.captures.is_empty());
    }

    #[test]
    fn destructured_parameter_capture_uses_element_types() {
        let function = parse_quote!(
            fn main((url, port): (String, u16)) {}
        );
        let result = prepare(
            &parse_quote!(|| (url.clone(), port)),
            &Bindings::from_function(&function),
            &parse_quote!(__captures),
        )
        .unwrap();
        assert_eq!(capture_types(&result), ["String", "u16"]);
    }

    fn capture_types(plan: &Plan) -> Vec<String> {
        plan.captures
            .iter()
            .map(|(_, ty)| ty.to_token_stream().to_string())
            .collect()
    }

    #[test]
    fn nested_tuple_and_array_locals_preserve_each_binding_type() {
        let mut bindings = Bindings::default();
        bindings.observe_statement(&parse_quote!(
            let ((url, port), [first, _, last]): ((String, u16), [u8; 3]) = inputs;
        ));
        let result = prepare(
            &parse_quote!(|| (url.clone(), port, first, last)),
            &bindings,
            &parse_quote!(__captures),
        )
        .unwrap();
        assert_eq!(capture_types(&result), ["String", "u16", "u8", "u8"]);
    }

    #[test]
    fn tuple_alias_helpers_follow_authored_arity_without_exposing_alias_names() {
        let wildcards = (0..63).map(|_| quote!(_)).collect::<Vec<_>>();
        let statement: Stmt =
            syn::parse_quote!(let (value, #(#wildcards,)*): PrivateAlias = input;);
        let mut bindings = Bindings::default();
        bindings.observe_statement(&statement);
        let plan = prepare(
            &parse_quote!(|| value),
            &bindings,
            &parse_quote!(__captures),
        )
        .unwrap();
        assert!(plan.requires_record);
        let helpers = bindings.projection_helpers();
        let file: syn::File = syn::parse2(helpers.clone()).unwrap();
        assert!(file.items.iter().any(|item| matches!(item, Item::Impl(item) if matches!(&*item.self_ty, Type::Tuple(tuple) if tuple.elems.len() == 64))));
        assert!(!helpers.to_string().contains("PrivateAlias"));
    }

    #[test]
    fn sequence_helpers_do_not_expose_private_alias_names() {
        let mut bindings = Bindings::default();
        bindings.observe_statement(&parse_quote!(let [head, tail @ ..]: PrivateArray = input;));
        let plan = prepare(
            &parse_quote!(|| (head, tail.len())),
            &bindings,
            &parse_quote!(__captures),
        )
        .unwrap();
        assert!(plan.requires_record);
        let helpers = bindings.projection_helpers();
        let _: syn::File = syn::parse2(helpers.clone()).unwrap();
        assert!(!helpers.to_string().contains("PrivateArray"));
        let types = capture_types(&plan);
        assert!(types[0].contains("__SystasisCaptureElement"));
        assert!(types[1].contains("__SystasisCaptureArrayTail"));
    }

    #[test]
    fn closure_globs_affect_only_their_lexical_scopes() {
        let mut bindings = Bindings::default();
        bindings.observe_statement(&parse_quote!(let outside: String = input;));
        let plan = prepare(
            &parse_quote!(|| {
                let from_import = {
                    use module::*;
                    answer()
                };
                outside.len() + from_import
            }),
            &bindings,
            &parse_quote!(__captures),
        )
        .unwrap();
        assert_eq!(capture_types(&plan), ["String"]);
        let error = prepare(
            &parse_quote!(|| {
                use module::*;
                { outside.len() }
            }),
            &bindings,
            &parse_quote!(__captures),
        )
        .err()
        .unwrap();
        assert!(
            error
                .to_string()
                .contains("glob import shadows the referenced outer binding")
        );
    }

    #[test]
    fn tuple_rest_aligns_suffix_types_and_array_rest_skips_elements() {
        let mut bindings = Bindings::default();
        bindings.observe_statement(&parse_quote!(
            let (first, .., last): (u8, String, bool, u64) = tuple;
        ));
        bindings.observe_statement(&parse_quote!(
            let [start, .., end]: [u16; COUNT] = array;
        ));
        let result = prepare(
            &parse_quote!(|| (first, last, start, end)),
            &bindings,
            &parse_quote!(__captures),
        )
        .unwrap();
        assert_eq!(capture_types(&result), ["u8", "u64", "u16", "u16"]);
    }

    #[test]
    fn unsupported_patterns_remain_untyped_instead_of_guessing() {
        let statements: Vec<Stmt> = vec![
            parse_quote!(let (value, ..): Alias = input;),
            parse_quote!(let [_, value @ ..]: [u8; COUNT] = input;),
            parse_quote!(let [_, _, value @ ..]: [u8; 1] = input;),
            parse_quote!(let [value, _]: [u8; 1] = input;),
            parse_quote!(let Record { value }: Record = input;),
            parse_quote!(let (value, _, _): (u8, u16) = input;),
        ];
        for statement in statements {
            let mut bindings = Bindings::from_function(&parse_quote!(
                fn context<const COUNT: usize>() {}
            ));
            bindings.observe_statement(&parse_quote!(let value: OldType = input;));
            bindings.observe_statement(&statement);
            let error = prepare(
                &parse_quote!(|| value),
                &bindings,
                &parse_quote!(__captures),
            )
            .err()
            .expect("unsupported destructuring must not retain an earlier annotation");
            assert!(error.to_string().contains("explicit type annotation"));
        }
    }

    #[test]
    fn slice_rest_types_preserve_lengths_and_binding_modes() {
        let cases: Vec<(Stmt, &str)> = vec![
            (
                parse_quote!(let [_, value @ ..]: [u8; 3] = input;),
                "[u8 ; 2]",
            ),
            (
                parse_quote!(let [value @ .., _]: &[u8; 3] = input;),
                "& '_ [u8 ; 2]",
            ),
            (
                parse_quote!(let [_, value @ .., _]: &mut [u8; 3] = input;),
                "& '_ mut [u8 ; 1]",
            ),
            (parse_quote!(let [value @ ..]: [u8; N] = input;), "[u8 ; N]"),
            (
                parse_quote!(let [_, value @ ..]: &[u8] = input;),
                "& '_ [u8]",
            ),
            (
                parse_quote!(let [value @ ..]: &mut [u8] = input;),
                "& '_ mut [u8]",
            ),
            (
                parse_quote!(let [ref value @ ..]: [u8; 3] = input;),
                "& '_ [u8 ; 3]",
            ),
        ];
        for (statement, expected) in cases {
            let mut bindings = Bindings::default();
            bindings.observe_statement(&statement);
            let result = prepare(
                &parse_quote!(|| value),
                &bindings,
                &parse_quote!(__captures),
            )
            .unwrap();
            assert_eq!(capture_types(&result), [expected]);
        }
    }

    #[test]
    fn concrete_lengths_preserve_expressions_without_generic_subtraction() {
        let mut bindings = Bindings::from_function(&parse_quote!(
            fn context<T, const N: usize>() {}
        ));
        let expressions: [Expr; 3] = [
            parse_quote!(N),
            parse_quote!(N + 1),
            parse_quote!(T::LENGTH),
        ];
        for expression in expressions {
            bindings
                .observe_statement(&parse_quote!(let [_, tail @ ..]: [u8; #expression] = input;));
            assert!(bindings.types["tail"].is_none());
        }
        bindings.observe_statement(
            &parse_quote!(let [_, tail @ ..]: [u8; module::LENGTH + 1] = input;),
        );
        assert_eq!(
            bindings.types["tail"]
                .as_ref()
                .unwrap()
                .to_token_stream()
                .to_string(),
            "[u8 ; (module :: LENGTH + 1) - 1]"
        );
    }

    #[test]
    fn explicit_reference_patterns_and_ref_bindings_keep_reference_types() {
        let mut bindings = Bindings::default();
        bindings.observe_statement(
            &parse_quote!(let (ref value, ref mut other): (String, u8) = input;),
        );
        bindings.observe_statement(&parse_quote!(let &(copied, _): &(u32, u8) = input;));
        let result = prepare(
            &parse_quote!(|| (value, other, copied)),
            &bindings,
            &parse_quote!(__captures),
        )
        .unwrap();
        assert_eq!(
            capture_types(&result),
            ["& '_ String", "& '_ mut u8", "u32"]
        );
    }

    #[test]
    fn implicit_binding_modes_follow_reference_layers_without_changing_siblings() {
        let mut bindings = Bindings::default();
        bindings.observe_statement(&parse_quote!(
            let ((shared,), mutable): &mut (&(u32,), u64) = input;
        ));
        bindings.observe_statement(&parse_quote!(
            let ([first, .., last],): &mut ([u8; 3],) = input;
        ));
        let result = prepare(
            &parse_quote!(|| (shared, mutable, first, last)),
            &bindings,
            &parse_quote!(__captures),
        )
        .unwrap();
        assert_eq!(
            capture_types(&result),
            ["& '_ u32", "& '_ mut u64", "& '_ mut u8", "& '_ mut u8"]
        );
    }

    #[test]
    fn repeated_reference_layers_preserve_shared_and_exclusive_binding_modes() {
        let mut bindings = Bindings::default();
        bindings.observe_statement(&parse_quote!(let (shared,): &&mut (u32,) = input;));
        bindings.observe_statement(&parse_quote!(let (exclusive,): &mut &mut (u16,) = input;));
        bindings.observe_statement(&parse_quote!(let (reference,): &(&str,) = input;));
        let result = prepare(
            &parse_quote!(|| (shared, exclusive, reference)),
            &bindings,
            &parse_quote!(__captures),
        )
        .unwrap();
        assert_eq!(
            capture_types(&result),
            ["& '_ u32", "& '_ mut u16", "& '_ & str"]
        );
    }
}
