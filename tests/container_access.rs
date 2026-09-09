//! Checked generated access retains locks and preserves cloning semantics.
#![forbid(unsafe_code)]

use systasis::app_container::Error;

macro_rules! scenario {
    ($module:ident, ($($requirements:tt)*)) => {
        mod $module {
            use super::*;
            trait IValue {}
            impl IValue for String {}
            trait ILength {}
            impl ILength for usize {}
            #[systasis::container($($requirements)*)]
            #[test]
            fn temporary_build_borrows_and_runtime_contention() -> Result<(), Error> {
                let built = systasis_container! {
                    register_value!({
                        try_resolve_ref_mut!(IValue)?.push('!');
                        try_resolve_ref!(IValue)?.len()
                    }: usize as ILength);
                    register_value!(String::from("data"): String as IValue);
                }.build::<Error>();
                let container = built?;
                assert_eq!(container.resolve_i_length(), 5);
                assert_eq!(container.try_resolve_i_value_clone()?, "data!");
                let reader = container.try_resolve_i_value_ref()?;
                assert!(matches!(container.try_resolve_i_value(), Err(Error::ValueAccessContention)));
                assert!(matches!(container.try_resolve_i_value_ref_mut(), Err(Error::ValueAccessContention)));
                drop(reader);
                {
                    let mut writer = container.try_resolve_i_value_ref_mut()?;
                    assert!(matches!(container.try_resolve_i_value_clone(), Err(Error::ValueAccessContention)));
                    writer.push('?');
                }
                assert_eq!(container.try_resolve_i_value()?, "data!?");
                assert!(matches!(container.try_resolve_i_value_clone(), Err(Error::ValueAlreadyConsumed)));
                Ok(())
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
        let Ok(container) = systasis_container! {
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
        let Ok(container) = systasis_container! {
            register_value!(Value: Value as IValue);
        }
        .build();
        let _ = container.resolve_i_value();
        assert_eq!(CLONES.load(Ordering::Relaxed), 0);
        let _ = container.resolve_i_value_clone();
        assert_eq!(CLONES.load(Ordering::Relaxed), 1);
    }
}
