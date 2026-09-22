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

    fn receive(container: &SystasisContainer) -> String {
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
        assert_eq!(receive(&container), "inferred");
        assert_eq!(receive(&container), "inferred");
    }
}

mod inferred_capture_with_guard {
    use systasis::app_container::Error;

    struct View<'a>(systasis::RefMut<'a, String>, usize);
    trait IView {}
    impl IView for View<'_> {}
    trait IText {}
    impl IText for String {}

    #[systasis::container(require(Send, Sync))]
    #[test]
    fn checked_native_capture_preserves_returned_guard_lifetime() -> Result<(), Error> {
        // An inferred capture selects native storage without a caller macro.
        let label = String::from("label");
        let Ok(container) = systasis::systasis_container! {
            register_value!(String::from("before"): String as IText);
            register_type_with!(View<'_> as IView, try move || -> Result<View<'_>, Error> {
                Ok(View(try_resolve_ref_mut!(IText)?, label.len()))
            });
        }
        .build();
        let mut view = container.try_resolve_i_view()?;
        assert_eq!(view.1, 5);
        assert!(matches!(
            container.try_resolve_i_text_ref(),
            Err(Error::ValueAccessContention)
        ));
        view.0.push_str("-after");
        drop(view);
        assert_eq!(&*container.try_resolve_i_view()?.0, "before-after");
        Ok(())
    }
}

mod send_only {
    use core::cell::Cell;

    trait ICount {}
    impl ICount for u32 {}

    #[systasis::container(require(Send))]
    #[test]
    fn requested_send_does_not_also_require_sync() {
        let count = Cell::new(0_u32);
        let Ok(container) = systasis::systasis_container! {
            register_type_with!(u32 as ICount, move || {
                count.set(count.get() + 1);
                count.get()
            });
        }
        .build();
        assert_eq!(container.resolve_i_count(), 1);
        assert_eq!(container.resolve_i_count(), 2);
    }
}

mod no_requirements {
    use std::rc::Rc;

    trait IText {}
    impl IText for String {}

    #[systasis::container]
    #[test]
    fn local_capture_does_not_gain_thread_safety_requirements() {
        let text = Rc::new(String::from("local"));
        let Ok(container) = systasis::systasis_container! {
            register_type_with!(String as IText, move || text.as_ref().clone());
        }
        .build();
        assert_eq!(container.resolve_i_text(), "local");
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

    fn receive<T: Clone>(container: &SystasisContainer<T>) -> T {
        container.resolve_i_value()
    }

    #[systasis::container]
    fn run<T: Clone>(value: T) -> (T, T) {
        let capture = value;
        let Ok(container) = systasis::systasis_container! {
            register_type_with!(T as IValue, move || capture.clone());
        }
        .build();
        (receive(&container), receive(&container))
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
