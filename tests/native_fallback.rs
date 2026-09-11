//! Native capture inference handles source shapes reconstruction cannot name.
#![forbid(unsafe_code)]

mod destructured {
    use std::rc::Rc;

    struct Parts {
        text: String,
        local: Rc<()>,
    }
    trait IText {}
    impl IText for String {}

    #[systasis::container(require(Send, Sync))]
    #[test]
    fn struct_destructuring_captures_only_the_referenced_binding() {
        let Parts { text, local }: Parts = Parts {
            text: String::from("text"),
            local: Rc::new(()),
        };
        let Ok(container) = systasis::systasis_container! {
            register_type_with!(String as IText, move || text.clone());
        }
        .build();
        assert_eq!(container.resolve_i_text(), "text");
        assert_eq!(container.resolve_i_text(), "text");
        assert_eq!(Rc::strong_count(&local), 1);
    }
}

mod inferred {
    trait IText {}
    impl IText for String {}

    fn receive(container: &AppContainer) -> String {
        container.resolve_i_text()
    }

    #[systasis::container(require(Send, Sync))]
    #[test]
    fn unannotated_owned_binding_preserves_nameable_container() {
        let text = String::from("inferred");
        let Ok(container) = systasis::systasis_container! {
            register_type_with!(String as IText, move || text.clone());
        }
        .build();
        assert_eq!(receive(container), "inferred");
        assert_eq!(receive(container), "inferred");
    }
}

mod tuple {
    struct Parts(String, usize);
    trait IText {}
    impl IText for String {}

    #[systasis::container]
    #[test]
    fn tuple_struct_binding_keeps_native_capture_type() {
        let Parts(text, length): Parts = Parts(String::from("text"), 4);
        let Ok(container) = systasis::systasis_container! {
            register_type_with!(String as IText, move || text.clone());
        }
        .build();
        assert_eq!(container.resolve_i_text().len(), length);
    }
}

mod imported {
    trait IText {}
    impl IText for String {}
    mod source {
        pub fn text() -> String {
            String::from("text")
        }
    }

    #[systasis::container]
    #[test]
    fn function_local_glob_import_keeps_rust_name_resolution() {
        use source::*;
        let Ok(container) = systasis::systasis_container! {
            register_type_with!(String as IText, || text());
        }
        .build();
        assert_eq!(container.resolve_i_text(), "text");
    }
}

mod generic {
    trait IValue {}
    impl<T> IValue for T {}

    fn receive<T: Clone>(container: &AppContainer<T>) -> T {
        container.resolve_i_value()
    }

    #[systasis::container]
    fn run<T: Clone>(value: T) -> (T, T) {
        let capture = value;
        let Ok(container) = systasis::systasis_container! {
            register_type_with!(T as IValue, move || capture.clone());
        }
        .build();
        (receive(container), receive(container))
    }

    #[test]
    fn inferred_generic_capture_keeps_the_authored_type_parameter() {
        assert_eq!(
            run(String::from("generic")),
            ("generic".into(), "generic".into())
        );
        assert_eq!(run(42u32), (42, 42));
    }
}

mod introduced_binding {
    trait IText {}
    impl IText for String {}

    #[systasis::container]
    #[test]
    fn binding_introduced_by_a_macro_is_captured_after_expansion() {
        macro_rules! configure {
            ($binding:ident) => {
                let $binding: String = String::from("macro input");
            };
        }
        configure!(config);
        let Ok(container) = systasis::systasis_container! {
            register_type_with!(String as IText, move || config.clone());
        }
        .build();
        assert_eq!(container.resolve_i_text(), "macro input");
        assert_eq!(container.resolve_i_text(), "macro input");
    }
}

mod tuple_rest {
    type Inputs = (String, bool, u16);
    trait IText {}
    impl IText for String {}

    #[systasis::container]
    #[test]
    fn unknown_arity_tuple_alias_rest_keeps_uncaptured_values_local() {
        let (text, .., number): Inputs = (String::from("rest"), true, 7);
        let Ok(container) = systasis::systasis_container! {
            register_type_with!(String as IText, move || text.clone());
        }
        .build();
        assert_eq!(container.resolve_i_text(), "rest");
        assert_eq!(number, 7);
    }
}
