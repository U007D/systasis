//! Stored values resolve by copying, cloning, or one-time ownership transfer.
#![forbid(unsafe_code)]

mod values {
    trait ICopy {}
    trait IClone {}
    trait IOwned {}
    #[derive(Copy)]
    struct Copied(u8);
    #[allow(clippy::non_canonical_clone_impl)] // Prove the resolver copies, not clones.
    impl Clone for Copied {
        fn clone(&self) -> Self {
            panic!("copy resolution must not call Clone")
        }
    }
    impl ICopy for Copied {}
    #[derive(Clone)]
    struct Cloned(String);
    impl IClone for Cloned {}
    struct Owned(u8);
    impl IOwned for Owned {}

    #[systasis::container]
    fn init() -> SystasisContainer {
        let Ok(container) = systasis::systasis_container! {
            register_value!(Copied(7): Copied as ICopy);
            register_value!(Cloned(String::from("hello")): Cloned as IClone);
            register_value!(Owned(9): Owned as IOwned);
        }
        .build();
        container
    }

    #[test]
    fn copying_and_cloning_are_repeatable_and_try_aliases_are_infallible() {
        let container = init();
        assert_eq!(container.resolve_i_copy().0, 7);
        let Ok(copied) = container.try_resolve_i_copy();
        assert_eq!(copied.0, 7);
        let mut first = container.resolve_clone_i_clone();
        first.0.push('!');
        let Ok(second) = container.try_resolve_clone_i_clone();
        assert_eq!(second.0, "hello");
    }

    #[test]
    fn move_only_values_are_transferred_once() {
        let container = init();
        assert_eq!(container.try_resolve_i_owned().unwrap().0, 9);
        assert!(matches!(
            container.try_resolve_i_owned(),
            Err(systasis::container::Error::ValueAlreadyConsumed)
        ));
    }
}

mod references {
    trait IMutable {}
    impl IMutable for &mut u8 {}

    #[systasis::container]
    fn init<'a>(shared: &'a str, mutable: &'a mut u8) -> SystasisContainer<'a> {
        let Ok(container) = systasis::systasis_container! {
            register_value!(shared: &'a str as Copy);
            register_value!(mutable: &'a mut u8 as IMutable);
        }
        .build();
        container
    }

    #[test]
    #[allow(
        clippy::drop_non_drop,
        reason = "verify the transferred reference survives container destruction"
    )]
    fn references_are_values_and_mutable_references_are_not_reborrowed() {
        let mut value = 0;
        let text = String::from("borrowed");
        let container = init(&text, &mut value);
        assert_eq!(container.resolve_copy(), "borrowed");
        let Ok(shared) = container.try_resolve_copy();
        assert_eq!(shared, "borrowed");
        let mutable = container.try_resolve_i_mutable().unwrap();
        // No borrow of the container is retained by transferring its &mut value.
        assert_eq!(container.resolve_copy(), "borrowed");
        assert!(matches!(
            container.try_resolve_i_mutable(),
            Err(systasis::container::Error::ValueAlreadyConsumed)
        ));
        drop(container);
        *mutable = 4; // The transferred reference outlives the container.
        assert_eq!(value, 4);
    }
}

mod generic_clone {
    trait IValue {}
    impl<T> IValue for T {}

    #[systasis::container]
    fn init<T: Clone + IValue>(value: T) -> SystasisContainer<T> {
        let Ok(container) = systasis::systasis_container! {
            register_value!(value: T as IValue);
        }
        .build();
        container
    }

    #[test]
    fn clone_bound_selects_the_same_api_for_copy_and_noncopy_instantiations() {
        assert_eq!(init(7_u8).resolve_clone_i_value(), 7);
        let Ok(value) = init(String::from("value")).try_resolve_clone_i_value();
        assert_eq!(value, "value");
    }
}

mod generic_move {
    trait IValue {}
    impl<T> IValue for T {}

    #[systasis::container]
    fn init<T: IValue>(value: T) -> SystasisContainer<T> {
        let Ok(container) = systasis::systasis_container! {
            register_value!(value: T as IValue);
        }
        .build();
        container
    }

    #[test]
    fn no_copy_or_clone_bound_keeps_move_only_resolution() {
        let container = init(String::from("value"));
        assert_eq!(container.try_resolve_i_value().unwrap(), "value");
        assert!(container.try_resolve_i_value().is_err());
    }
}

mod child {
    pub trait IText {}
    impl IText for String {}
    systasis::systasis_container! {
        register_value!(String::from("child"): String as IText in named);
        register_value!(5: u8 as Copy);
        register_type!(String as IText in fresh);
        register_type_with!(u8 as Copy in constructed, || 7);
    }
}

mod parent {
    use super::child;
    trait IText {}
    impl IText for String {}
    #[systasis::container]
    fn check(primary: &child::SystasisContainer) {
        let Ok(container) = systasis::systasis_container! {
            register_container!(primary: &child::SystasisContainer);
            register_value!({ let Ok(text) = try_resolve_clone_from!(IText, primary::named); text }:
                resolve_type_from!(IText, primary::named) as IText);
            register_value!({ let Ok(value) = try_resolve_from!(Copy, primary); value }: u8 as Copy);
            register_value!({ let Ok(text) = try_resolve_clone!(IText); text }: String as IText in copied);
            register_value!({ let Ok(value) = try_resolve!(Copy); value }: u8 as Copy in copied);
        }
        .build();
        assert_eq!(container.resolve_clone_i_text(), "child");
        assert_eq!(container.resolve_clone_i_text_in_copied(), "child");
        assert_eq!(container.resolve_copy_in_copied(), 5);
        let Ok(text) = container.primary().try_resolve_clone_i_text_in_named();
        assert_eq!(text, "child");
        let Ok(number) = container.primary().try_resolve_copy();
        assert_eq!(number, 5);
        let Ok(fresh) = container.primary().try_resolve_i_text_in_fresh();
        assert!(fresh.is_empty());
        let Ok(constructed) = container.primary().try_resolve_copy_in_constructed();
        assert_eq!(constructed, 7);
    }

    #[test]
    fn named_child_paths_keep_copy_and_clone_policies() {
        let Ok(child) = child::SystasisContainer::build();
        check(&child);
    }
}
