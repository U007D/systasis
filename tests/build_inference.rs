//! Error inference and caller control without an enclosing Result-returning function.
#![forbid(unsafe_code)]

macro_rules! infallible {
    ($module:ident, $($error:tt)*) => {
        mod $module {
            trait IValue {}
            impl IValue for u32 {}
            #[systasis::container(require(Send, Sync))]
            #[test]
            fn inferred_uninhabited_error() {
                let Ok(container) = systasis::systasis_container! {
                    register_value!(1: u32 as IValue);
                }.build $($error)* ();
                assert_eq!(container.resolve_i_value(), 1);
            }
        }
    };
}
infallible!(implicit,);
infallible!(placeholder, ::<_>);
infallible!(explicit, ::<core::convert::Infallible>);

mod empty {
    #[systasis::container]
    #[test]
    fn empty_build_has_uninhabited_error() {
        let Ok(_container) = systasis::systasis_container! {}.build();
    }
}

mod never_type {
    #[systasis::container]
    #[test]
    fn unconstrained_error_is_never_not_unit_or_infallible() {
        fn error_name<C, E>(_: &Result<C, E>) -> &'static str {
            core::any::type_name::<E>()
        }
        let built = systasis::systasis_container! {}.build();
        assert_eq!(error_name(&built), "!");
    }
}

mod fallible {
    trait IValue {}
    impl IValue for String {}
    #[derive(Debug, PartialEq)]
    struct Failure;
    #[systasis::container]
    #[test]
    fn ordinary_result_annotation_controls_error_type() {
        let built: Result<SystasisContainer, Failure> = systasis::systasis_container! {
            register_value!(Err::<String, Failure>(Failure)?: String as IValue);
        }
        .build();
        let Err(error) = built else {
            panic!("initializer must fail")
        };
        assert_eq!(error, Failure);
    }
}

mod let_else {
    trait IValue {}
    impl IValue for String {}
    #[systasis::container]
    #[test]
    fn original_let_else_branch_is_preserved() {
        let Ok(_container) = systasis::systasis_container! {
            register_value!(Err::<String, ()>(())?: String as IValue);
        }
        .build::<()>() else {
            return;
        };
        panic!("failed build must execute the caller's else branch");
    }
}

mod propagated {
    trait IValue {}
    impl IValue for String {}

    #[systasis::container]
    fn run(fail: bool) -> Result<String, &'static str> {
        let container = systasis::systasis_container! {
            register_value!(
                if fail { Err("initialization failed")? } else { String::from("ready") }:
                String as IValue
            );
        }
        .build::<&'static str>()?;
        Ok(container.try_resolve_i_value().expect("value is available"))
    }

    #[test]
    fn question_mark_propagates_failure_and_preserves_owner_on_success() {
        assert_eq!(run(false), Ok(String::from("ready")));
        assert_eq!(run(true), Err("initialization failed"));
    }
}

mod chained {
    trait IValue {}
    impl IValue for String {}

    #[systasis::container]
    #[test]
    fn result_methods_keep_the_container_owner_in_scope() {
        let container = (systasis::systasis_container! {
            register_value!(String::from("ready"): String as IValue);
        }
        .build::<()>())
        .expect("infallible initialization");
        assert_eq!(&*container.try_resolve_i_value_ref().unwrap(), "ready");
    }
}
