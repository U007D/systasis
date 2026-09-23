//! Unchecked operations retain occupancy and nonblocking guard behavior.
#![cfg(feature = "resolve_unchecked")]

use systasis::container::Error;

trait IValue {}
impl IValue for String {}

#[systasis::container]
#[test]
fn generated_unchecked_access_retains_nonblocking_guards() {
    let value: String = "value".into();
    let Ok(container) = systasis::systasis_container! {
        register_value!(value: String as IValue);
    }
    .build();
    let scope = systasis::scoped::AsScope::<systasis::scoped::mask::Empty>::scope(&container);
    // SAFETY: the freshly built value is present and no incompatible guard exists.
    let read = unsafe { scope.resolve_i_value_ref_unchecked() };
    assert_eq!(&*read, "value");
    assert!(matches!(
        container.try_resolve_i_value(),
        Err(Error::ValueAccessContention)
    ));
    drop(read);
    // SAFETY: the shared guard was released; the value remains present.
    let mut write = unsafe { scope.resolve_i_value_ref_mut_unchecked() };
    write.push('!');
    assert!(matches!(
        container.try_resolve_i_value_ref(),
        Err(Error::ValueAccessContention)
    ));
    drop(write);
    // SAFETY: all guards are released and nothing has consumed the value.
    assert_eq!(unsafe { scope.resolve_i_value_unchecked() }, "value!");
    assert!(matches!(
        container.try_resolve_i_value(),
        Err(Error::ValueAlreadyConsumed)
    ));
}

mod local {
    use super::*;

    #[systasis::container(require(!Sync))]
    #[test]
    fn local_unchecked_guards_keep_refcell_borrows() {
        let value: String = "local".into();
        let Ok(container) = systasis::systasis_container! {
            register_value!(value: String as IValue in primary);
        }
        .build();
        // SAFETY: the value is present with no outstanding borrow.
        let mut write = unsafe { container.resolve_i_value_ref_mut_unchecked_in_primary() };
        write.push('!');
        assert!(matches!(
            container.try_resolve_i_value_ref_in_primary(),
            Err(Error::ValueAccessContention)
        ));
        drop(write);
        // SAFETY: the exclusive guard was released and the value is present.
        assert_eq!(
            unsafe { container.resolve_i_value_unchecked_in_primary() },
            "local!"
        );
    }
}

mod queries {
    use super::*;
    trait ILength {}
    impl ILength for usize {}

    #[systasis::container]
    #[test]
    fn unsafe_queries_preserve_caller_context_and_dependency_order() {
        let value: String = "ordered".into();
        let Ok(container) = systasis::systasis_container! {
            // SAFETY: this initializer is the only accessor and the dependency
            // DAG initializes the value first.
            register_value!(unsafe { resolve_unchecked_from!(IValue, primary) }.len(): usize as ILength);
            register_value!(value: String as IValue in primary);
        }.build();
        assert_eq!(container.resolve_i_length(), 7);
        assert!(matches!(
            container.try_resolve_i_value_in_primary(),
            Err(Error::ValueAlreadyConsumed)
        ));
    }
}

mod forbid_consumer {
    #![forbid(unsafe_code)]
    use super::*;

    #[systasis::container]
    #[test]
    fn checked_consumers_can_enable_the_feature_with_forbid_unsafe() {
        let value: String = "checked".into();
        let Ok(container) = systasis::systasis_container! {
            register_value!(value: String as IValue);
        }
        .build();
        assert_eq!(container.try_resolve_i_value().unwrap(), "checked");
    }
}

mod returned_guard {
    use super::*;
    trait IGuard {}
    impl IGuard for systasis::Ref<'_, String> {}

    #[systasis::container]
    #[test]
    fn lazy_constructor_retains_the_unchecked_shared_guard() {
        let value: String = "guarded".into();
        let Ok(container) = systasis::systasis_container! {
            register_value!(value: String as IValue);
            register_type_with!(systasis::Ref<'_, String> as IGuard, || {
                // SAFETY: owned access is excluded by this constructor borrow;
                // this test releases every mutable guard before resolving it.
                unsafe { resolve_ref_unchecked!(IValue) }
            });
        }
        .build();
        let guard = container.resolve_i_guard();
        assert_eq!(&*guard, "guarded");
        assert!(matches!(
            container.try_resolve_i_value_ref_mut(),
            Err(Error::ValueAccessContention)
        ));
        drop(guard);
        container.try_resolve_i_value_ref_mut().unwrap().push('!');
        assert_eq!(&*container.resolve_i_guard(), "guarded!");
    }
}
