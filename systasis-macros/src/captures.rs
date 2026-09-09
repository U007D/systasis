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
}

impl Bindings {
    pub(crate) fn from_function(function: &ItemFn) -> Self {
        let mut bindings = Self::default();
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
                if matches!(item, Item::Use(_) | Item::Macro(_)) {
                    bindings.uncertain = Some(item.to_token_stream());
                }
            }
        }
        bindings
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
        // A tuple/struct annotation describes the whole pattern, not each
        // binding. Ref patterns likewise change the binding's annotated type.
        let direct = matches!(pattern, Pat::Ident(binding)
            if binding.by_ref.is_none() && binding.subpat.is_none());
        for name in names {
            self.types
                .insert(name, annotation.filter(|_| direct).cloned());
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
            self.reject(ident, "captured constructor bindings require an explicit type annotation on a simple binding");
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
                if matches!(item, Item::Use(_) | Item::Macro(_)) {
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
                Stmt::Macro(invocation) => self.reject(invocation, "constructor capture analysis cannot inspect opaque macros; compute macro inputs in explicitly typed bindings before the container"),
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
            Expr::Macro(invocation) => self.reject(invocation, "constructor capture analysis cannot inspect opaque macros; compute macro inputs in explicitly typed bindings before the container"),
            Expr::Verbatim(tokens) => self.reject(tokens, "constructor capture analysis cannot inspect this syntax"),
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
    fn destructured_capture_does_not_reuse_entire_pattern_type() {
        let function = parse_quote!(
            fn main((url, port): (String, u16)) {}
        );
        let error = prepare(
            &parse_quote!(|| url.clone()),
            &Bindings::from_function(&function),
            &parse_quote!(__captures),
        )
        .err()
        .expect("destructured type extraction is unsupported");
        assert!(error.to_string().contains("simple binding"));
    }
}
