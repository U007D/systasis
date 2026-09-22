//! Temporary annotations retain borrowed array captures extracted from tuple aliases.
#![forbid(unsafe_code)]
#![allow(clippy::redundant_at_rest_pattern)]

mod generic_fields {
    use std::{cell::Cell, rc::Rc};

    type Input<T, U> = ([T; 2], U);
    trait ILength {}
    impl ILength for usize {}

    struct Uncaptured(Rc<Cell<usize>>);
    impl Drop for Uncaptured {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }

    fn receive<T: Sync, U>(container: &AppContainer<'_, T, U>) -> usize {
        container.resolve_i_length()
    }

    #[systasis::container(require(Send, Sync))]
    fn run<T: Sync, U>(input: Input<T, U>, drops: &Cell<usize>) {
        let ([ref values @ ..], unused): Input<T, U> = input;
        let values: &[T; 2] = values;

        let Ok(container) = systasis::systasis_container! {
            register_type_with!(usize as ILength, move || {
                let exact: &[T; 2] = values;
                exact.len()
            });
        }
        .build();

        drop(unused);
        assert_eq!(drops.get(), 1);
        assert_eq!(receive(&container), 2);
        assert_eq!(receive(&container), 2);
        assert!(core::ptr::eq(values, &input.0));
    }

    #[test]
    fn only_captured_field_constrains_ownership_and_auto_traits() {
        let values = [String::from("first"), String::from("second")];
        let drops = Rc::new(Cell::new(0));
        run(
            ([&values[0], &values[1]], Uncaptured(drops.clone())),
            &drops,
        );
        assert_eq!(drops.get(), 1);
    }
}

mod external_elements {
    type Input<'a, T> = ([&'a T; 2], bool);
    trait IElement {}
    impl<T> IElement for &T {}

    #[systasis::container]
    fn run<'a, T>(input: Input<'a, T>) -> &'a T {
        let ([ref values @ ..], _): Input<'a, T> = input;
        let values: &[&'a T; 2] = values;

        let Ok(container) = systasis::systasis_container! {
            register_type_with!(&'a T as IElement, move || values[0]);
        }
        .build();

        let first = container.resolve_i_element();
        assert!(core::ptr::eq(first, container.resolve_i_element()));
        first
    }

    #[test]
    fn returned_external_reference_outlives_container_without_extra_bound() {
        let values = [String::from("first"), String::from("second")];
        assert!(core::ptr::eq(
            run(([&values[0], &values[1]], false)),
            &values[0]
        ));
    }
}
