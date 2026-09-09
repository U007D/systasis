//! Bounded, deterministic token mutation; no compiler-success claim for emitted code.

use proc_macro2::{Group, TokenStream, TokenTree};
use quote::quote;

const FIXTURES: &[&str] = &[
    "fn main() { let Ok(c) = systasis_container! {}.build(); }",
    "fn main() { let Ok(c) = systasis_container! { register_value!(42: u32 as IValue); }.build(); }",
    "fn main() { let Ok(c) = systasis_container! { register_type!(Value as IValue); }.build(); }",
    "fn main(input: u32) { let Ok(c) = systasis_container! { register_type_with!(u32 as IValue, move || input); }.build(); }",
    "fn main<T: Copy>(input: T) { let Ok(c) = systasis_container! { register_value!(input: T as IValue); }.build(); }",
    "fn main() { let Ok(c) = systasis_container! { register_value!(Value: Value as dyn IValue); }.build(); }",
    "fn main() { let Ok(c) = systasis_container! { register_value!(Value: Value as IWrite + IRead in named); }.build(); }",
    "fn main(child: &Child) { let Ok(c) = systasis_container! { register_container!(primary: &Child); }.build(); }",
    "fn main() { let c = systasis_container! { register_value!(value()?: Value as IValue); }.build::<Error>()?; }",
    "fn main() { let c = systasis_container! { register_value!(try_resolve!(ISource)?: registered_type!(ISource) as ITarget); register_value!(Value: Value as ISource); }.build::<Error>()?; }",
];

/// Fixed algorithm and seed keep a failed mutation reproducible on every host.
fn next(state: &mut u64) -> usize {
    *state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
    (*state >> 32) as usize
}

fn mutate(tokens: TokenStream, state: &mut u64, depth: usize) -> TokenStream {
    let mut tokens = tokens.into_iter().collect::<Vec<_>>();
    if tokens.is_empty() {
        return quote!(unexpected);
    }
    let index = next(state) % tokens.len();
    if depth < 8
        && let TokenTree::Group(group) = &tokens[index]
        && next(state).is_multiple_of(2)
    {
        let mut replacement =
            Group::new(group.delimiter(), mutate(group.stream(), state, depth + 1));
        replacement.set_span(group.span());
        tokens[index] = TokenTree::Group(replacement);
    } else {
        match next(state) % 3 {
            0 => {
                tokens.remove(index);
            }
            1 => tokens.insert(index, tokens[index].clone()),
            _ => {
                let replacements = [quote!(unexpected), quote!(0), quote!(;), quote!(::)];
                tokens.splice(
                    index..=index,
                    replacements[next(state) % replacements.len()].clone(),
                );
            }
        }
    }
    tokens.into_iter().collect()
}

fn expand(tokens: TokenStream) -> syn::Result<TokenStream> {
    let function = syn::parse2(tokens)?;
    crate::generate::expand(function, false, &[])
}

fn outcome(tokens: TokenStream) -> Result<String, String> {
    expand(tokens)
        .map(|output| {
            syn::parse2::<syn::File>(output.clone())
                .unwrap_or_else(|error| panic!("invalid emitted Rust: {error}\n{output}"));
            output.to_string()
        })
        .map_err(|error| error.into_compile_error().to_string())
}

#[test]
fn accepted_fixture_expansions_are_repeatable_rust_syntax() {
    for source in FIXTURES {
        let tokens: TokenStream = source.parse().unwrap();
        let first = outcome(tokens.clone()).unwrap_or_else(|error| panic!("{source}\n{error}"));
        assert_eq!(outcome(tokens).unwrap(), first, "{source}");
    }
}

fn check_mutations(cases_per_fixture: usize) {
    let mut state = 0x7379_7374_6173_6973;
    let mut accepted = 0;
    let mut rejected = 0;
    for source in FIXTURES {
        let original: TokenStream = source.parse().unwrap();
        for case in 0..cases_per_fixture {
            let tokens = mutate(original.clone(), &mut state, 0);
            let first = outcome(tokens.clone());
            assert_eq!(outcome(tokens.clone()), first, "case {case}: {tokens}");
            if first.is_ok() {
                accepted += 1;
            } else {
                rejected += 1;
            }
        }
    }
    // Controls prevent a corpus change from silently exercising only one path.
    assert!(accepted > 0, "no generated output checked");
    assert!(rejected > 0, "no error path checked");
}

#[test]
fn sampled_mutations_return_repeatable_diagnostics_or_rust_syntax() {
    check_mutations(16);
}

#[test]
#[ignore = "larger deterministic mutation corpus; run explicitly for parser regression work"]
fn extended_mutation_corpus() {
    check_mutations(256);
}
