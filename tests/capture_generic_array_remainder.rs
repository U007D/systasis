//! Generic array-alias remainders retain their exact type without body macros.
#![forbid(unsafe_code)]

use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

type Input<T> = [T; 3];
trait ILength {}
impl ILength for usize {}

struct Tracked(Arc<AtomicUsize>);

impl Drop for Tracked {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

#[systasis::container(require(Send, Sync))]
fn run<T: Send + Sync>(input: Input<T>) {
    let [head, tail @ ..]: Input<T> = input;
    let Ok(container) = systasis::systasis_container! {
        register_type_with!(usize as ILength, || {
            let exact: &[T; 2] = &tail;
            exact.len()
        });
    }
    .build();
    // Only the remainder is captured: the head remains independently owned.
    drop(head);
    assert_eq!(container.resolve_i_length(), 2);
    assert_eq!(container.resolve_i_length(), 2);
}

#[test]
fn plain_constructor_keeps_exact_generic_remainder_and_drops_each_element_once() {
    let drops: [Arc<AtomicUsize>; 3] = std::array::from_fn(|_| Arc::new(AtomicUsize::new(0)));
    run(std::array::from_fn(|index| Tracked(drops[index].clone())));
    assert_eq!(drops.map(|count| count.load(Ordering::SeqCst)), [1, 1, 1]);
}

mod owned_capture_lending {
    trait IText {}
    impl IText for &str {}

    #[systasis::container]
    #[test]
    fn ordinary_owned_capture_can_still_lend_its_contents() {
        let text: String = String::from("owned capture");
        let original: *const u8 = text.as_ptr();
        let Ok(container) = systasis::systasis_container! {
            register_type_with!(&'_ str as IText, move || text.as_str());
        }
        .build();
        let first = container.resolve_i_text();
        let second = container.resolve_i_text();
        assert_eq!(first, "owned capture");
        assert!(core::ptr::eq(first.as_ptr(), original));
        assert!(core::ptr::eq(first.as_ptr(), second.as_ptr()));
    }
}

mod borrowed_slice_with_owned_capture {
    type Input<T> = T;
    trait IText {}
    impl IText for &str {}

    #[systasis::container]
    fn run<'a, T>(input: &'a [T]) {
        let [head, tail @ ..]: Input<&'a [T]> = input else {
            return;
        };
        let text: String = String::from("owned alongside borrowed");
        let original: *const u8 = text.as_ptr();
        let Ok(container) = systasis::systasis_container! {
            register_type_with!(&'_ str as IText, move || {
                let _: &[T] = tail;
                text.as_str()
            });
        }
        .build();
        assert!(core::ptr::eq(head, &input[0]));
        let first = container.resolve_i_text();
        let second = container.resolve_i_text();
        assert_eq!(first, "owned alongside borrowed");
        assert!(core::ptr::eq(first.as_ptr(), original));
        assert!(core::ptr::eq(first.as_ptr(), second.as_ptr()));
    }

    #[test]
    fn generic_slice_projection_preserves_lending_from_another_capture() {
        run(&[1_u8, 2, 3]);
    }
}

mod hidden_static_borrow {
    type Input<T> = &'static [T];
    trait IText {}
    impl IText for &str {}

    #[systasis::container]
    fn run<T: 'static>(input: Input<T>) {
        let [_, tail @ ..]: Input<T> = input else {
            return;
        };
        let text: String = String::from("owned beside hidden reference");
        let Ok(container) = systasis::systasis_container! {
            register_type_with!(&'_ str as IText, move || {
                let _: &[T] = tail;
                text.as_str()
            });
        }
        .build();
        assert_eq!(container.resolve_i_text(), "owned beside hidden reference");
    }

    #[test]
    fn alias_without_lifetime_arguments_can_still_hide_a_borrowed_slice() {
        run(&[1_u8, 2, 3]);
    }
}
