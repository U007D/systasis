//! Generated implementation details must not change caller name or type resolution.
#![forbid(unsafe_code)]

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
        .build::<systasis::app_container::Error>();

        assert_eq!(
            built.unwrap().try_resolve_i_value().unwrap(),
            "caller input"
        );
    }
}

mod caller_result_alias {
    type Result<T> = core::result::Result<T, systasis::app_container::Error>;

    trait IValue {}
    impl IValue for String {}

    #[systasis::container]
    #[test]
    fn generated_results_do_not_use_the_callers_alias() -> Result<()> {
        let built = systasis::systasis_container! {
            register_value!(String::from("value"): String as IValue);
        }
        .build::<systasis::app_container::Error>();

        assert_eq!(built?.try_resolve_i_value()?, "value");
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
        .build::<systasis::app_container::Error>();

        let value: Box<[u8]> = built.unwrap().try_resolve_i_value().unwrap();
        assert_eq!(&*value, &[1]);
    }
}
