//! Array and slice rest captures retain their annotated element/reference types.
#![forbid(unsafe_code)]
use std::cell::Cell;
trait ILength {}
impl ILength for usize {}
const LENGTH: usize = 3;

struct Tracked<'a>(&'a Cell<usize>);
impl Drop for Tracked<'_> {
    fn drop(&mut self) {
        self.0.set(self.0.get() + 1);
    }
}

mod owned {
    use super::*;
    #[systasis::container]
    fn run(drops: &Cell<usize>) {
        let [head, tail @ ..]: [Tracked<'_>; 3] = [Tracked(drops), Tracked(drops), Tracked(drops)];
        let Ok(container) = systasis::systasis_container! {
            register_type_with!(usize as ILength, || tail.len());
        }
        .build();
        assert_eq!(container.resolve_i_length(), 2);
        assert_eq!(drops.get(), 0);
        drop(head);
        assert_eq!(drops.get(), 1);
    }
    #[test]
    fn only_selected_rest_is_moved_and_each_element_drops_once() {
        let drops = Cell::new(0);
        run(&drops);
        assert_eq!(drops.get(), 3);
    }
}

mod borrowed {
    use super::*;
    #[systasis::container]
    fn run(shared: &[u8; 3], exclusive: &mut [u8; 3], slice: &[u8], mutable_slice: &mut [u8]) {
        let [_, array_tail @ ..]: &[u8; LENGTH] = shared;
        let [_, mutable_tail @ ..]: &mut [u8; 1 + 2] = exclusive;
        let [_, slice_tail @ ..]: &[u8] = slice else {
            return;
        };
        let [_, mutable_slice_tail @ ..]: &mut [u8] = mutable_slice else {
            return;
        };
        let Ok(container) = systasis::systasis_container! {
            register_type_with!(usize as ILength, || array_tail.len() + mutable_tail.len() + slice_tail.len() + mutable_slice_tail.len());
        }.build();
        assert_eq!(container.resolve_i_length(), 7);
    }
    #[test]
    fn borrowed_array_and_slice_rest_capture_without_copying_payloads() {
        run(&[1, 2, 3], &mut [4, 5, 6], &[7, 8], &mut [9, 10, 11]);
    }
}
