//! Conditional ordinary source bindings are selected by rustc before capture analysis.
#![forbid(unsafe_code)]
// Always-true/false predicates deliberately exercise configuration selection.
#![allow(clippy::non_minimal_cfg)]

mod conflicting {
    trait IValue {}
    impl IValue for u32 {}

    fn ordinary() -> u32 {
        #[cfg(all())]
        let value: u32 = 3;
        #[cfg(any())]
        let value: String = String::new();
        let constructor = move || value;
        constructor()
    }

    #[systasis::container(require(Send, Sync))]
    #[test]
    fn inactive_annotation_does_not_replace_active_capture_type() {
        #[cfg(all())]
        let value: u32 = 3;
        #[cfg(any())]
        let value: String = String::new();
        let Ok(container) = systasis::systasis_container! {
            register_type_with!(u32 as IValue, move || value);
        }
        .build();
        fn guarantees<T: Send + Sync>(_: &T) {}
        guarantees(container);
        assert_eq!(container.resolve_i_value(), ordinary());
    }
}

mod nested {
    trait IValue {}
    impl IValue for u32 {}

    #[systasis::container]
    #[test]
    fn nested_blocks_cfg_attr_and_target_feature_predicates() {
        let value: u32 = {
            #[cfg_attr(all(), cfg_attr(all(), cfg(any())))]
            let value: Undefined = undefined;
            #[cfg_attr(any(), cfg(any()))]
            let value: u32 = 4;
            #[cfg(feature = "std")]
            let value: u32 = value + 10;
            #[cfg(target_pointer_width = "64")]
            let value: u32 = value + 100;
            #[cfg(not(any(target_pointer_width = "32", target_pointer_width = "64")))]
            let value: Undefined = undefined;
            value
        };
        let Ok(container) = systasis::systasis_container! {
            register_type_with!(u32 as IValue, move || value);
        }
        .build();
        assert_eq!(
            container.resolve_i_value(),
            4 + if cfg!(feature = "std") { 10 } else { 0 }
                + if cfg!(target_pointer_width = "64") {
                    100
                } else {
                    0
                }
        );
    }
}

mod lifecycle {
    use core::cell::Cell;
    trait IValue {}
    impl IValue for usize {}
    trait IStored {}
    impl IStored for String {}
    struct State<'a>(&'a Cell<usize>, &'a Cell<usize>);
    impl Drop for State<'_> {
        fn drop(&mut self) {
            self.1.set(self.1.get() + 1);
        }
    }

    #[systasis::container(require(!Sync))]
    #[allow(unused_variables)]
    fn run(calls: &Cell<usize>, drops: &Cell<usize>) {
        #[cfg(all())]
        let state: State<'_> = State(calls, drops);
        #[cfg(any())]
        let state: String = String::new();
        let Ok(container) = systasis::systasis_container! {
            register_value!(String::from("stored"): String as IStored);
            register_type_with!(usize as IValue, move || {
                state.0.set(state.0.get() + 1);
                state.0.get()
            });
        }
        .build();
        assert_eq!(calls.get(), 0);
        assert_eq!(container.resolve_i_value(), 1);
        assert_eq!(container.resolve_i_value(), 2);
        assert_eq!(drops.get(), 0);
        let guard: core::cell::Ref<'_, String> = container.try_resolve_i_stored_ref().unwrap();
        assert_eq!(&*guard, "stored");
    }

    #[test]
    fn preserves_lazy_constructor_ownership_and_local_policy() {
        let calls = Cell::new(0);
        let drops = Cell::new(0);
        run(&calls, &drops);
        assert_eq!(drops.get(), 1);
    }
}

#[cfg(any())]
#[systasis::container]
fn wholly_discarded_function() {
    undefined!();
}

mod ancestor {
    trait IValue {}
    impl IValue for u32 {}

    fn ordinary() -> u32 {
        #[cfg(any())]
        {
            #[cfg(invalid_predicate(foo))]
            let ignored: u8 = missing;
        }
        3
    }

    #[systasis::container]
    #[test]
    fn inactive_ancestors_do_not_evaluate_descendant_predicates() {
        #[cfg(any())]
        {
            #[cfg(invalid_predicate(foo))]
            let ignored: u8 = missing;
        }
        #[cfg_attr(all(), cfg(any()))]
        if undefined {
            #[cfg(invalid_predicate(foo))]
            let ignored: u8 = missing;
        }
        let values: [u32; 1] = [
            #[cfg(any())]
            {
                #[cfg(invalid_predicate(foo))]
                let ignored: u8 = missing;
                undefined
            },
            3,
        ];
        let value: u32 = match values[0] {
            #[cfg(any())]
            _ => {
                #[cfg(invalid_predicate(foo))]
                let ignored: u8 = missing;
                undefined
            }
            value => value,
        };
        let Ok(container) = systasis::systasis_container! {
            register_type_with!(u32 as IValue, move || value);
        }
        .build();
        assert_eq!(container.resolve_i_value(), ordinary());
    }
}
