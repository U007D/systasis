//! Array and slice alias projections preserve exact captured field types.
#![forbid(unsafe_code)]
trait IValue {}
impl IValue for usize {}
mod owned {
    use super::*;
    type Input = [String; 3];
    #[systasis::container]
    #[test]
    fn run() {
        let input: Input = ["head".into(), "second".into(), "third".into()];
        let [head, tail @ ..]: Input = input;
        let Ok(c) = systasis::systasis_container! {
            register_type_with!(usize as IValue, || tail.len());
        }
        .build();
        assert_eq!(head, "head");
        assert_eq!(c.resolve_i_value(), 2);
    }
}
mod shared {
    use super::*;
    type Input<'a, T> = &'a [T];
    trait ISequence {}
    impl<T> ISequence for &[T] {}
    fn receive<'a, T>(container: &AppContainer<'a, T>) -> &'a [T] {
        container.resolve_i_sequence()
    }
    #[systasis::container]
    fn run<'a, T>(input: Input<'a, T>) {
        let [head, tail @ ..]: Input<'a, T> = input else {
            return;
        };
        let Ok(c) = systasis::systasis_container! {
            register_type_with!(usize as IValue, || tail.len() + core::mem::size_of_val(head));
            register_type_with!(&'a [T] as ISequence, || tail);
        }
        .build();
        assert_eq!(c.resolve_i_value(), 3);
        assert!(core::ptr::eq(receive(c), &input[1..]));
    }
    #[test]
    fn check() {
        run(&[1_u8, 2, 3]);
    }
}

mod mutable {
    use super::*;
    type Input<'a, T> = &'a mut [T];
    #[systasis::container]
    fn run<'a, T>(input: Input<'a, T>) {
        let [head, tail @ ..]: Input<'a, T> = input else {
            return;
        };
        let Ok(c) = systasis::systasis_container! {
            register_type_with!(usize as IValue, || tail.len());
        }
        .build();
        let _: &mut T = head;
        assert_eq!(c.resolve_i_value(), 2);
    }
    #[test]
    fn check() {
        run(&mut [1_u8, 2, 3]);
    }
}

mod generic_array_element {
    use super::*;
    type Input<T> = [T; 3];
    #[systasis::container]
    fn run<T>(input: Input<T>) {
        let [head, tail, _]: Input<T> = input;
        let Ok(c) = systasis::systasis_container! {
            register_type_with!(usize as IValue, || core::mem::size_of_val(&head));
        }
        .build();
        drop(tail);
        assert_eq!(c.resolve_i_value(), core::mem::size_of::<T>());
    }
    #[test]
    fn check() {
        run([String::new(), String::new(), String::new()]);
    }
}

mod destruction {
    use super::*;
    use core::sync::atomic::{AtomicUsize, Ordering};
    static DROPS: AtomicUsize = AtomicUsize::new(0);
    struct Value;
    impl Drop for Value {
        fn drop(&mut self) {
            DROPS.fetch_add(1, Ordering::Relaxed);
        }
    }
    type Input = [Value; 3];
    #[systasis::container]
    fn run(input: Input) {
        let [head, tail @ ..]: Input = input;
        let Ok(c) = systasis::systasis_container! {
            register_type_with!(usize as IValue, || tail.len());
        }
        .build();
        drop(head);
        assert_eq!(DROPS.load(Ordering::Relaxed), 1);
        assert_eq!(c.resolve_i_value(), 2);
        assert_eq!(c.resolve_i_value(), 2);
    }
    #[test]
    fn owned_remainder_drops_once_and_unselected_head_stays_local() {
        run([Value, Value, Value]);
        assert_eq!(DROPS.load(Ordering::Relaxed), 3);
    }
}
