//! Explicit parameter types need no added lifetime annotations for capture storage.
#![forbid(unsafe_code)]
use core::cell::Cell;

mod single {
    use super::*;
    trait ICount {}
    impl ICount for usize {}
    #[systasis::container]
    fn run(calls: &Cell<usize>) {
        let Ok(container) = systasis::systasis_container! {
            register_type_with!(usize as ICount, move || { calls.set(calls.get() + 1); calls.get() });
        }.build();
        assert_eq!(container.resolve_i_count(), 1);
        assert_eq!(container.resolve_i_count(), 2);
    }
    #[test]
    fn captured_parameter_reference_uses_inferred_backing_lifetime() {
        let calls = Cell::new(0);
        run(&calls);
        assert_eq!(calls.get(), 2);
    }
}

mod multiple {
    trait IView {}
    impl IView for &str {}
    trait ILength {}
    impl ILength for usize {}
    #[systasis::container]
    fn run(left: &str, right: Option<&[u8]>) {
        let Ok(container) = systasis::systasis_container! {
            register_type_with!(&'_ str as IView, move || left);
            register_type_with!(usize as ILength, move || right.map_or(0, |bytes| bytes.len()));
        }
        .build();
        assert_eq!(container.resolve_i_view(), left);
        assert_eq!(
            container.resolve_i_length(),
            right.map_or(0, |bytes| bytes.len())
        );
    }
    #[test]
    fn independent_references_and_nested_generic_capture_types_are_lifted() {
        let left = String::from("longer lived");
        {
            let right = [1, 2, 3];
            run(&left, Some(&right));
        }
        assert_eq!(left, "longer lived");
    }
}

mod callable {
    trait ILength {}
    impl ILength for usize {}
    #[systasis::container]
    fn run(callback: fn(&str) -> usize, text: &str) {
        let Ok(container) = systasis::systasis_container! {
            register_type_with!(usize as ILength, move || callback(text));
        }
        .build();
        assert_eq!(container.resolve_i_length(), text.len());
    }
    #[test]
    fn function_pointer_elision_remains_higher_ranked() {
        run(str::len, "hello");
    }
}

mod callable_trait {
    trait ILength {}
    impl ILength for usize {}
    #[systasis::container]
    fn run(callback: &dyn Fn(&str) -> usize, text: &str) {
        let Ok(container) = systasis::systasis_container! {
            register_type_with!(usize as ILength, move || callback(text));
        }
        .build();
        assert_eq!(container.resolve_i_length(), text.len());
    }
    #[test]
    fn callable_trait_binders_remain_local() {
        let callback = |text: &str| text.len();
        run(&callback, "borrowed callback");
    }
}
