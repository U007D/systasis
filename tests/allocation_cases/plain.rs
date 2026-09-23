//! Plain storage checks; caller Clone implementations deliberately do no allocation.

use super::{assert_no_allocations, measure};
use core::{cell::Cell, hint::black_box};

#[derive(Copy)]
struct Counted<'a>(&'a Cell<usize>);

#[allow(clippy::non_canonical_clone_impl)]
impl Clone for Counted<'_> {
    fn clone(&self) -> Self {
        self.0.set(self.0.get() + 1);
        *self
    }
}

trait ICounted {}
impl ICounted for Counted<'_> {}

#[systasis::container]
fn copy_and_clone(calls: &Cell<usize>) {
    let Ok(container) = systasis::systasis_container! {
        register_value!(Counted(calls): Counted<'_> as ICounted);
    }
    .build();
    black_box(container.resolve_i_counted());
    let Ok(copied) = container.try_resolve_i_counted();
    black_box(copied);
    assert_eq!(calls.get(), 0);
}

#[test]
fn copy_resolution_does_not_allocate_or_invoke_clone() {
    let calls = Cell::new(0);
    let (_, counts) = measure(|| copy_and_clone(&calls));
    assert_no_allocations(counts);
    assert_eq!(calls.get(), 0);
}

#[test]
fn read_only_runtime_storage_and_clone_do_not_allocate() {
    let calls = Cell::new(0);
    let (_, counts) = measure(|| {
        // This verifies the primitive, not a generated read-only registration.
        let slot = systasis::__private::ReadSlot::new(Counted(&calls));
        black_box(slot.resolve_ref());
        assert_eq!(calls.get(), 0);
        black_box(slot.resolve_clone());
        assert_eq!(calls.get(), 1);
    });
    assert_no_allocations(counts);
    assert_eq!(calls.get(), 1);
}
