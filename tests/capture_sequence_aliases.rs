//! Array and slice alias projections preserve exact captured field types.
#![forbid(unsafe_code)]
mod borrowed_arrays {
    use super::*;
    type Array = [String; 3];
    #[systasis::container]
    fn run(input: &mut Array) {
        let [head, tail @ ..]: &mut Array = input;
        let Ok(c) = systasis::systasis_container! {
            register_type_with!(usize as IValue, || {
                let exact: &[String; 2] = tail;
                exact.len()
            });
        }
        .build();
        head.push('!');
        assert_eq!(c.resolve_i_value(), 2);
    }
    #[test]
    fn mutable_array_alias_preserves_exact_tail_length() {
        let mut values = ["one".into(), "two".into(), "three".into()];
        run(&mut values);
        assert_eq!(values[0], "one!");
    }
}
mod unsized_slice_alias {
    use super::*;
    type Slice = [String];
    #[systasis::container]
    fn run(input: &Slice) {
        let [head, tail @ ..]: &Slice = input else {
            return;
        };
        let Ok(c) = systasis::systasis_container! {
            register_type_with!(usize as IValue, || tail.len() + head.len());
        }
        .build();
        assert_eq!(c.resolve_i_value(), 5);
    }
    #[test]
    fn borrowed_unsized_alias_remains_a_slice() {
        run(&["one".into(), "two".into(), "three".into()]);
    }
}
trait IValue {}
impl IValue for usize {}
mod reference_slice_aliases {
    use super::*;
    type Shared = &'static [u8];
    type Exclusive = &'static mut [u8];
    #[systasis::container]
    fn run(shared: Shared, exclusive: Exclusive) {
        let [head, tail @ ..]: Shared = shared else {
            return;
        };
        // Exercise a tail projection even though this particular tail is whole.
        #[allow(clippy::redundant_at_rest_pattern)]
        let [remainder @ ..]: Exclusive = exclusive;
        let Ok(container) = systasis::systasis_container! {
            register_type_with!(usize as IValue, || {
                let shared_tail: &[u8] = tail;
                let exclusive_tail: &[u8] = remainder;
                shared_tail.len() + exclusive_tail.len()
            });
        }
        .build();
        assert_eq!(*head, 1);
        assert_eq!(container.resolve_i_value(), 2);
    }
    #[test]
    fn argument_free_aliases_preserve_shared_and_exclusive_slice_tails() {
        // Empty mutable storage can have a static lifetime without heap leaking.
        run(&[1, 2, 3], &mut []);
    }
}
mod owned {
    use super::*;
    type Input = [String; 3];
    #[systasis::container]
    #[test]
    fn owned_array_alias_retains_exact_remainder() {
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
    fn generic_shared_slice_returns_original_tail_from_named_container() {
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
    fn generic_mutable_slice_preserves_disjoint_head_access() {
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
    fn generic_array_element_moves_without_copy_bound() {
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
