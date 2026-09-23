//! Generated implementation details must not change caller name or type resolution.
#![forbid(unsafe_code)]

mod raw_interface {
    #[allow(non_camel_case_types)]
    trait r#type {}
    impl r#type for u32 {}
    #[systasis::container]
    #[test]
    fn raw_trait_identifier_generates_a_valid_method() {
        let Ok(container) = systasis::systasis_container! {
            register_value!(7_u32: u32 as r#type);
        }
        .build();
        assert_eq!(container.resolve_type(), 7);
    }
}

mod caller_local {
    trait IValue {}
    impl IValue for String {}

    #[systasis::container]
    #[test]
    fn initializer_keeps_its_original_binding() {
        let __systasis_error = String::from("caller input");
        let built = systasis::systasis_container! {
            register_value!(__systasis_error: String as IValue);
        }
        .build::<systasis::container::Error>();

        assert_eq!(built.unwrap().resolve_i_value_clone(), "caller input");
    }
}

mod caller_result_alias {
    type Result<T> = core::result::Result<T, systasis::container::Error>;

    trait IValue {}
    impl IValue for String {}

    #[systasis::container]
    #[test]
    fn generated_results_do_not_use_the_callers_alias() -> Result<()> {
        let built = systasis::systasis_container! {
            register_value!(String::from("value"): String as IValue);
        }
        .build::<systasis::container::Error>();

        assert_eq!(built?.resolve_i_value_clone(), "value");
        Ok(())
    }
}

mod annotated_coercion {
    trait IValue {}
    impl IValue for Box<[u8]> {}

    #[systasis::container]
    #[test]
    fn declared_type_supplies_initializer_coercion_context() {
        let built = systasis::systasis_container! {
            register_value!(Box::new([1_u8]): Box<[u8]> as IValue);
        }
        .build::<systasis::container::Error>();

        let value: Box<[u8]> = built.unwrap().resolve_i_value_clone();
        assert_eq!(&*value, &[1]);
    }
}
