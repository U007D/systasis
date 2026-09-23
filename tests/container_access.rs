//! Stored Copy/Clone values are immutable; callers own every resolved result.
#![forbid(unsafe_code)]

macro_rules! scenario {
    ($module:ident, ($($requirements:tt)*)) => {
        mod $module {
            trait IValue {}
            impl IValue for String {}
            trait ILength {}
            impl ILength for usize {}
            #[systasis::container($($requirements)*)]
            #[test]
            fn cloned_dependencies_do_not_modify_stored_values() {
                let Ok(container) = systasis::systasis_container! {
                    register_value!({
                        let mut value = resolve_clone!(IValue);
                        value.push('!');
                        value.len()
                    }: usize as ILength);
                    register_value!(String::from("data"): String as IValue);
                }.build();
                assert_eq!(container.resolve_i_length(), 5);
                let Ok(mut first) = container.try_resolve_i_value_clone();
                first.push('?');
                assert_eq!(first, "data?");
                assert_eq!(container.resolve_i_value_clone(), "data");
                let Ok(second) = container.try_resolve_i_value_clone();
                assert_eq!(second, "data");
            }
        }
    };
}
scenario!(synchronized, (require(Send, Sync)));
scenario!(local, (require(!Sync)));

mod external_copy_reference {
    #[derive(Clone, Copy)]
    struct Label<'a>(&'a str);
    trait ILabel {}
    impl ILabel for Label<'_> {}
    #[systasis::container]
    #[test]
    fn lifetime_placeholder_does_not_remove_copy_behavior() {
        let text = String::from("label");
        let Ok(container) = systasis::systasis_container! {
            register_value!(Label(&text): Label<'_> as ILabel);
        }
        .build();
        assert_eq!(container.resolve_i_label().0, "label");
        assert_eq!(container.resolve_i_label().0, "label");
    }
}

mod explicit_copy_clone {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static CLONES: AtomicUsize = AtomicUsize::new(0);
    #[derive(Copy)]
    struct Value;
    #[allow(clippy::non_canonical_clone_impl)]
    impl Clone for Value {
        fn clone(&self) -> Self {
            CLONES.fetch_add(1, Ordering::Relaxed);
            *self
        }
    }
    trait IValue {}
    impl IValue for Value {}
    #[systasis::container]
    #[test]
    fn copy_resolution_does_not_clone() {
        let Ok(container) = systasis::systasis_container! {
            register_value!(Value: Value as IValue);
        }
        .build();
        let _ = container.resolve_i_value();
        assert_eq!(CLONES.load(Ordering::Relaxed), 0);
        let Ok(_value) = container.try_resolve_i_value();
        assert_eq!(CLONES.load(Ordering::Relaxed), 0);
    }
}
