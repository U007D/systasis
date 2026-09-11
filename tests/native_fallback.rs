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
