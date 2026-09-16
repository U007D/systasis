//! A whole-sequence pattern preserves its exact type, ownership and borrows.
#![forbid(unsafe_code)]
#![allow(clippy::redundant_at_rest_pattern)]

mod owned {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    type Input<T> = [T; 2];
    trait ISequence {}
    impl<T, const N: usize> ISequence for &[T; N] {}

    struct Tracked(Arc<AtomicUsize>);
    impl Drop for Tracked {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::Relaxed);
        }
    }

    fn receive<T: Send + Sync>(container: &AppContainer<T>) -> usize {
        let first: &[T; 2] = container.resolve_i_sequence();
        let second: &[T; 2] = container.resolve_i_sequence();
        assert!(core::ptr::eq(first, second));
        first.len()
    }

    #[systasis::container(require(Send, Sync))]
    fn run<T: Send + Sync>(input: Input<T>) -> usize {
        let [whole @ ..]: Input<T> = input;
        let Ok(container) = systasis::systasis_container! {
            register_type_with!(&'_ [T; 2] as ISequence, move || &whole);
        }
        .build();
        receive(container)
    }

    #[test]
    fn whole_generic_array_lends_its_exact_type_and_drops_once() {
        let drops = Arc::new(AtomicUsize::new(0));
        assert_eq!(run([Tracked(drops.clone()), Tracked(drops.clone())]), 2);
        assert_eq!(drops.load(Ordering::Relaxed), 2);
    }
}

mod borrowed_elements {
    type Input<'a, T> = [&'a T; 2];
    trait ISequence {}
    impl<T> ISequence for &[&T; 2] {}

    #[systasis::container(require(Send, Sync))]
    fn run<'a, T: Sync>(input: Input<'a, T>) {
        let [whole @ ..]: Input<'a, T> = input;
        let Ok(container) = systasis::systasis_container! {
            register_type_with!(&'_ [&'a T; 2] as ISequence, move || &whole);
        }
        .build();
        let first: &[&'a T; 2] = container.resolve_i_sequence();
        let second: &[&'a T; 2] = container.resolve_i_sequence();
        assert!(core::ptr::eq(first, second));
        assert!(core::ptr::eq(first[0], input[0]));
        assert!(core::ptr::eq(first[1], input[1]));
    }

    #[test]
    fn whole_array_retains_element_lifetimes_without_extra_outlives_bound() {
        let values = [String::from("first"), String::from("second")];
        run([&values[0], &values[1]]);
    }
}

mod external_shared {
    type Input<'a, T> = &'a [T; 2];
    trait ISequence {}
    impl<T> ISequence for &[T; 2] {}

    fn receive<'a, T>(container: &AppContainer<'a, T>) -> &'a [T; 2] {
        container.resolve_i_sequence()
    }

    #[systasis::container]
    fn run<'a, T>(input: Input<'a, T>) -> &'a [T; 2] {
        let [whole @ ..]: Input<'a, T> = input;
        let Ok(container) = systasis::systasis_container! {
            register_type_with!(&'a [T; 2] as ISequence, move || whole);
        }
        .build();
        receive(container)
    }

    #[test]
    fn reference_into_external_array_can_outlive_container() {
        let values = [String::from("first"), String::from("second")];
        assert!(core::ptr::eq(run(&values), &values));
    }
}

mod nested_shared_mutable {
    type Input<'outer, 'inner, T> = &'outer &'inner mut [T; 2];
    trait ISequence {}
    impl<T> ISequence for &[T; 2] {}

    #[systasis::container]
    fn run<'outer, 'inner, T>(input: Input<'outer, 'inner, T>) -> &'outer [T; 2] {
        let [whole @ ..]: Input<'outer, 'inner, T> = input;
        let Ok(container) = systasis::systasis_container! {
            register_type_with!(&'outer [T; 2] as ISequence, move || whole);
        }
        .build();
        container.resolve_i_sequence()
    }

    #[test]
    fn shared_outer_borrow_keeps_inner_mutable_array_shared() {
        let mut values = [String::from("first"), String::from("second")];
        let exclusive = &mut values;
        assert!(core::ptr::eq(run(&exclusive), &*exclusive));
    }
}

mod nested_mutable {
    use core::cell::Cell;
    type Input<'outer, 'inner, T> = &'outer mut &'inner mut [T; 2];
    trait ILength {}
    impl ILength for usize {}

    trait Increment {
        fn increment(&self);
    }
    impl Increment for Cell<u8> {
        fn increment(&self) {
            self.set(self.get() + 1);
        }
    }

    #[systasis::container(require(Send))]
    fn run<'outer, 'inner, T: Increment + Send>(input: Input<'outer, 'inner, T>) {
        let [whole @ ..]: Input<'outer, 'inner, T> = input;
        let Ok(container) = systasis::systasis_container! {
            register_type_with!(usize as ILength, move || {
                whole[0].increment();
                let exact: &[T; 2] = whole;
                exact.len()
            });
        }
        .build();
        assert_eq!(container.resolve_i_length(), 2);
        assert_eq!(container.resolve_i_length(), 2);
    }

    #[test]
    fn nested_exclusive_array_capture_is_send_without_requiring_sync() {
        let mut values = [Cell::new(1_u8), Cell::new(2)];
        run(&mut &mut values);
        assert_eq!(values.map(Cell::into_inner), [3, 2]);
    }
}

mod empty_array {
    type Input<T> = [T; 0];
    trait ISequence {}
    impl<T> ISequence for &[T; 0] {}

    #[systasis::container]
    fn run<T>([whole @ ..]: Input<T>) {
        let Ok(container) = systasis::systasis_container! {
            register_type_with!(&'_ [T; 0] as ISequence, move || &whole);
        }
        .build();
        assert!(container.resolve_i_sequence().is_empty());
    }

    #[test]
    fn empty_generic_array_parameter_preserves_zero_length() {
        run::<String>([]);
    }
}

mod whole_slice {
    type Input<'a, T> = &'a [T];
    trait IText {}
    impl IText for &str {}

    #[systasis::container]
    fn run<'a, T>(input: Input<'a, T>) {
        let [whole @ ..]: Input<'a, T> = input;
        let text: String = String::from("owned beside slice");
        let Ok(container) = systasis::systasis_container! {
            register_type_with!(&'_ str as IText, move || {
                let _: &[T] = whole;
                text.as_str()
            });
        }
        .build();
        let first = container.resolve_i_text();
        let second = container.resolve_i_text();
        assert_eq!(first, "owned beside slice");
        assert!(core::ptr::eq(first, second));
    }

    #[test]
    fn whole_slice_capture_preserves_borrowing_from_another_owned_capture() {
        run::<String>(&[]);
        run(&[String::from("external")]);
    }
}
