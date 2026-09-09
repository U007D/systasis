//! Group identity and public naming use different, deliberate orderings.
#![forbid(unsafe_code)]

mod first {
    pub trait IZebra {
        fn zebra(&self) -> u32;
    }
    impl IZebra for u32 {
        fn zebra(&self) -> u32 {
            *self
        }
    }
}
mod last {
    pub trait IAlpha {
        type Item;
        fn alpha(&self) -> Self::Item;
    }
    impl IAlpha for u32 {
        type Item = u32;
        fn alpha(&self) -> u32 {
            *self
        }
    }
}

mod qualified {
    use super::*;
    trait IOutput {}
    impl IOutput for u32 {}

    #[systasis::container]
    #[test]
    fn method_names_sort_trait_names_not_module_paths() {
        let Ok(container) = systasis::systasis_container! {
            register_value!(42: u32 as first::IZebra + last::IAlpha<Item = u32>);
            register_value!({
                let value: resolve_type!(last::IAlpha<Item = u32> + first::IZebra) =
                    resolve!(first::IZebra + last::IAlpha<Item = u32>);
                first::IZebra::zebra(&value) + last::IAlpha::alpha(&value)
            }: u32 as IOutput);
        }
        .build();
        assert_eq!(container.resolve_i_alpha_i_zebra(), 42);
        assert_eq!(container.resolve_i_output(), 84);
    }
}

macro_rules! permutation {
    ($module:ident, $a:ident, $b:ident, $c:ident) => {
        mod $module {
            trait IA {}
            trait IB {}
            trait IC {}
            impl IA for u32 {}
            impl IB for u32 {}
            impl IC for u32 {}
            #[systasis::container]
            #[test]
            fn every_permutation_selects_the_same_override() {
                let Ok(container) = systasis::systasis_container! {
                    register_value!(undefined(): Missing as IA + IB + IC);
                    register_value!(17: u32 as $a + $b + $c);
                }
                .build();
                assert_eq!(container.resolve_i_a_i_b_i_c(), 17);
            }
        }
    };
}
permutation!(abc, IA, IB, IC);
permutation!(acb, IA, IC, IB);
permutation!(bac, IB, IA, IC);
permutation!(bca, IB, IC, IA);
permutation!(cab, IC, IA, IB);
permutation!(cba, IC, IB, IA);
