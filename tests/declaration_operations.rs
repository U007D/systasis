//! Ordinary resolution and composition through the declaration API.
#![forbid(unsafe_code)]

mod stored {
    #[derive(Clone)]
    struct Value(u8);
    trait IValue {}
    impl IValue for Value {}

    systasis::systasis_container! {
        register_value!(self::Value(7): self::Value as IValue);
    }

    #[test]
    fn read_write_clone_and_consume_keep_the_same_slot() {
        use systasis::container::Error;
        let Ok(container) = SystasisContainer::build();
        let reader = container.try_resolve_i_value_ref().unwrap();
        assert_eq!(reader.0, 7);
        assert!(matches!(
            container.try_resolve_i_value(),
            Err(Error::ValueAccessContention)
        ));
        drop(reader);
        container.try_resolve_i_value_ref_mut().unwrap().0 = 8;
        assert_eq!(container.try_resolve_i_value_clone().unwrap().0, 8);
        assert_eq!(container.try_resolve_i_value().unwrap().0, 8);
        assert!(matches!(
            container.try_resolve_i_value_ref(),
            Err(Error::ValueAlreadyConsumed)
        ));
    }
}

mod fresh_and_named {
    trait IValue {
        fn number(&self) -> u8;
    }
    #[derive(Clone, Copy, Default)]
    struct Value(u8);
    impl IValue for Value {
        fn number(&self) -> u8 {
            self.0
        }
    }

    systasis::systasis_container! {
        register_type!(Value as IValue);
        register_value!(Value(42): Value as dyn IValue in named);
    }

    #[test]
    fn default_constructors_and_named_dyn_access_work() {
        let Ok(container) = SystasisContainer::build();
        assert_eq!(container.resolve_i_value().number(), 0);
        assert_eq!(container.resolve_i_value_in_named().number(), 42);
        assert_eq!(container.resolve_i_value_dyn_ref_in_named().number(), 42);
    }
}

mod leaf {
    trait IValue {}
    impl IValue for u8 {}
    systasis::systasis_container! {
        register_value!(42: u8 as IValue);
    }
}

mod parent {
    use super::leaf;
    trait ITotal {}
    impl ITotal for u8 {}

    #[systasis::container]
    fn verify(primary: &leaf::SystasisContainer) {
        let Ok(container) = systasis::systasis_container! {
            register_container!(primary: &leaf::SystasisContainer);
            register_value!(resolve_from!(IValue, primary) + 1: u8 as ITotal);
        }
        .build();
        fn scoped_value(child: &primary::SubContainer<'_>) -> u8 {
            child.resolve_i_value()
        }
        assert_eq!(scoped_value(container.primary()), 42);
        assert_eq!(container.resolve_i_total(), 43);
    }

    #[test]
    fn a_declaration_container_composes_with_the_attribute_form() {
        let Ok(child) = leaf::SystasisContainer::build();
        verify(&child);
    }
}
