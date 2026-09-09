//! Preserve source-relative paths when moving source into one generated module.

use syn::{
    Ident, ItemMod, Path, UseTree, parse_quote,
    visit_mut::{self, VisitMut},
};

#[derive(Default)]
pub(crate) struct Rebase {
    nested_modules: usize,
}

impl Rebase {
    fn use_tree(&self, tree: &mut UseTree) {
        if let UseTree::Group(group) = tree {
            for item in &mut group.items {
                self.use_tree(item);
            }
            return;
        }
        let mut current = &*tree;
        let mut parents = 0;
        while let UseTree::Path(path) = current {
            if path.ident != "super" {
                break;
            }
            parents += 1;
            current = &path.tree;
        }
        if parents > 0 && parents >= self.nested_modules {
            *tree = parse_quote!(super::#tree);
        } else if self.nested_modules == 0
            && let UseTree::Path(path) = tree
            && path.ident == "self"
        {
            path.ident = Ident::new("super", path.ident.span());
        }
    }
}

impl VisitMut for Rebase {
    fn visit_item_mod_mut(&mut self, module: &mut ItemMod) {
        self.nested_modules += 1;
        visit_mut::visit_item_mod_mut(self, module);
        self.nested_modules -= 1;
    }

    fn visit_path_mut(&mut self, path: &mut Path) {
        visit_mut::visit_path_mut(self, path);
        if path.leading_colon.is_some() {
            return;
        }
        let parents = path
            .segments
            .iter()
            .take_while(|segment| segment.ident == "super")
            .count();
        if parents > 0 && parents >= self.nested_modules {
            path.segments.insert(0, parse_quote!(super));
        } else if self.nested_modules == 0
            && let Some(first) = path.segments.first_mut()
            && first.ident == "self"
        {
            first.ident = Ident::new("super", first.ident.span());
        }
    }

    fn visit_item_use_mut(&mut self, import: &mut syn::ItemUse) {
        if import.leading_colon.is_some() {
            return;
        }
        self.use_tree(&mut import.tree);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use quote::ToTokens;

    #[test]
    fn source_type_paths_cross_one_additional_module() {
        let mut ty: syn::Type = parse_quote!(self::Wrapper<super::Input, crate::Stable, ::core::marker::PhantomData<self::T>>);
        Rebase::default().visit_type_mut(&mut ty);
        let expected: syn::Type = parse_quote!(super::Wrapper<super::super::Input, crate::Stable, ::core::marker::PhantomData<super::T>>);
        assert_eq!(
            ty.to_token_stream().to_string(),
            expected.to_token_stream().to_string()
        );
    }

    #[test]
    fn nested_module_local_paths_keep_their_meaning() {
        let mut block: syn::Block = parse_quote!({
            use self::Input as Local;
            mod nested {
                use self::Own;
                type Local = self::Own;
                type Parent = super::Input;
                mod child {
                    type Local = super::Own;
                    type Outer = super::super::Input;
                }
            }
        });
        Rebase::default().visit_block_mut(&mut block);
        let expected: syn::Block = parse_quote!({
            use super::Input as Local;
            mod nested {
                use self::Own;
                type Local = self::Own;
                type Parent = super::super::Input;
                mod child {
                    type Local = super::Own;
                    type Outer = super::super::super::Input;
                }
            }
        });
        assert_eq!(
            block.to_token_stream().to_string(),
            expected.to_token_stream().to_string()
        );
    }

    #[test]
    fn root_import_groups_rebase_each_branch_but_not_nested_self_imports() {
        let mut import: syn::ItemUse = parse_quote!(
            use {
                self::Input,
                super::Other,
                crate::fixed::{self, Value},
            };
        );
        Rebase::default().visit_item_use_mut(&mut import);
        let expected: syn::ItemUse = parse_quote!(
            use {
                super::Input,
                super::super::Other,
                crate::fixed::{self, Value},
            };
        );
        assert_eq!(
            import.to_token_stream().to_string(),
            expected.to_token_stream().to_string()
        );
    }
}
