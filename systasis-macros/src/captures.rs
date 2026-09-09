//! Lexical capture storage for repeatable constructors.
//!
//! Capture types come from explicit source annotations, never expression
//! inference. Rust checks whether the rewritten constructor can run through a
//! shared reference to its owned capture tuple.

use std::collections::{BTreeMap, BTreeSet};

use quote::ToTokens;
use syn::{
    Expr, ExprClosure, FnArg, Ident, Item, ItemFn, Pat, Stmt, Type,
    visit_mut::{self, VisitMut},
};

#[derive(Clone, Default)]
pub(crate) struct Bindings {
    types: BTreeMap<String, Option<Type>>,
    uncertain: Option<proc_macro2::TokenStream>,
    imports: Vec<syn::ItemUse>,
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
        let mut bindings = Self::default();
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
    /// Opaque aliases and struct fields still need unavailable type information.
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
                for element in &pattern.elems {
                    // A `tail @ ..` binding is an array, not an element. Its
                    // length would require evaluating/subtracting consts.
                    self.observe_annotated_pattern(element, &ty.elem, mode);
                }
            }
            _ => {}
        }
    }
}

pub(crate) struct Plan {
    pub(crate) captures: Vec<(Ident, Type)>,
    pub(crate) closure: ExprClosure,
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
        captures: Vec::new(),
        error: None,
    };
    visitor.visit_expr_closure_mut(&mut closure);
    if let Some(error) = visitor.error {
        return Err(error);
    }
    Ok(Plan {
        captures: visitor.captures,
        closure,
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
        for input in &closure.inputs {
            self.bind(input);
        }
        self.visit_expr_mut(&mut closure.body);
        self.scopes.pop();
    }

    fn visit_block_mut(&mut self, block: &mut syn::Block) {
        let mut scope = BTreeSet::new();
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
                    if !import_names(&import.tree, &mut imported)
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
            parse_quote!(let (value, _): Alias = input;),
            parse_quote!(let [_, value @ ..]: [u8; 3] = input;),
            parse_quote!(let Record { value }: Record = input;),
            parse_quote!(let (value, _, _): (u8, u16) = input;),
        ];
        for statement in statements {
            let mut bindings = Bindings::default();
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
