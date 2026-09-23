//! Checked storage behavior and retained-guard safety regressions.
#![forbid(unsafe_code)]

use std::{
    cell::Cell,
    rc::Rc,
    sync::{
        Arc, Barrier,
        atomic::{AtomicUsize, Ordering},
    },
};
use systasis::{
    __private::{CopySlot, TakeSlot},
    Ref, RefMut,
    container::Error,
};

fn assert_consumed<T>(result: Result<T, Error>) {
    assert!(matches!(result, Err(Error::ValueAlreadyConsumed)));
}

fn assert_contended<T>(result: Result<T, Error>) {
    assert!(matches!(result, Err(Error::ValueAccessContention)));
}

#[test]
fn consumption_is_permanent_for_every_checked_operation() {
    let slot = TakeSlot::new(String::from("value"));
    assert_eq!(slot.try_resolve().unwrap(), "value");
    assert_consumed(slot.try_resolve());
    assert_consumed(slot.try_resolve_ref());
    assert_consumed(slot.try_resolve_ref_mut());
    assert_consumed(slot.try_resolve_clone());
    // Failed occupancy checks must release their acquired locks.
    assert_consumed(slot.try_resolve_ref_mut());
}

#[test]
fn readers_coexist_and_prevent_consumption_or_mutation() {
    let slot = TakeSlot::new(String::from("value"));
    let first = slot.try_resolve_ref().unwrap();
    let second = slot.try_resolve_ref().unwrap();
    assert_eq!(&*first, &*second);
    assert_eq!(slot.try_resolve_clone().unwrap(), "value");
    assert_contended(slot.try_resolve());
    assert_contended(slot.try_resolve_ref_mut());
    drop(first);
    assert_contended(slot.try_resolve());
    drop(second);
    assert_eq!(slot.try_resolve().unwrap(), "value");
}

#[test]
fn writer_excludes_every_other_operation_until_dropped() {
    let slot = TakeSlot::new(String::from("value"));
    let mut writer = slot.try_resolve_ref_mut().unwrap();
    writer.push('!');
    assert_eq!(&*writer, "value!");
    assert_contended(slot.try_resolve());
    assert_contended(slot.try_resolve_ref());
    assert_contended(slot.try_resolve_ref_mut());
    assert_contended(slot.try_resolve_clone());
    drop(writer);
    assert_eq!(slot.try_resolve().unwrap(), "value!");
}

#[test]
fn projection_retains_lock_through_repeated_unsized_mapping() {
    let slot = TakeSlot::new(String::from("value"));
    let text: Ref<'_, str> = Ref::map(slot.try_resolve_ref().unwrap(), String::as_str);
    let bytes: Ref<'_, [u8]> = Ref::map(text, str::as_bytes);
    let suffix: Ref<'_, [u8]> = Ref::map(bytes, |bytes| &bytes[1..]);
    assert_eq!(&*suffix, b"alue");
    assert_contended(slot.try_resolve());
    drop(suffix);
    assert_eq!(slot.try_resolve().unwrap(), "value");
}

#[test]
fn trait_object_projection_preserves_access() {
    trait Label {
        fn label(&self) -> &str;
    }
    impl Label for String {
        fn label(&self) -> &str {
            self
        }
    }
    let slot = TakeSlot::new(String::from("value"));
    let view: Ref<'_, dyn Label> = Ref::map(slot.try_resolve_ref().unwrap(), |v| v as &dyn Label);
    assert_eq!(view.label(), "value");
    assert_contended(slot.try_resolve_ref_mut());
}

#[test]
fn static_projection_still_retains_original_lock() {
    let slot = TakeSlot::new(String::from("value"));
    let text = Ref::map(slot.try_resolve_ref().unwrap(), |_| "static");
    assert_eq!(&*text, "static");
    assert_contended(slot.try_resolve());
    drop(text);
    assert_eq!(slot.try_resolve().unwrap(), "value");
}

#[test]
fn consumable_storage_is_send_without_requiring_sync() {
    fn assert_send<T: Send>() {}
    assert_send::<TakeSlot<Cell<u32>>>();
    let slot = TakeSlot::new(Cell::new(7_u32));
    let result = std::thread::spawn(move || slot.try_resolve().unwrap().get());
    assert_eq!(result.join().unwrap(), 7);
}

#[test]
fn moving_guards_does_not_release_the_lock() {
    let slot = TakeSlot::new(4_u32);
    let read = slot.try_resolve_ref().unwrap();
    let read = Box::new(read);
    assert_eq!(**read, 4);
    assert_contended(slot.try_resolve());
    drop(read);
    let mut write = Box::new(slot.try_resolve_ref_mut().unwrap());
    **write += 1;
    assert_contended(slot.try_resolve());
    drop(write);
    assert_eq!(slot.try_resolve().unwrap(), 5);
}

#[test]
fn zero_sized_and_overaligned_values_remain_valid() {
    #[repr(align(256))]
    struct Aligned(u32);
    let zero = TakeSlot::new(());
    assert_eq!(*zero.try_resolve_ref().unwrap(), ());
    assert_eq!(*zero.try_resolve_ref_mut().unwrap(), ());
    zero.try_resolve().unwrap();
    assert_consumed(zero.try_resolve());

    let aligned = TakeSlot::new(Aligned(7));
    let mut guard = aligned.try_resolve_ref_mut().unwrap();
    assert_eq!((&*guard as *const Aligned).addr() % 256, 0);
    guard.0 += 1;
    drop(guard);
    assert_eq!(aligned.try_resolve().unwrap().0, 8);
}

#[test]
fn local_non_send_values_need_no_thread_bounds() {
    let value = Rc::new(Cell::new(7));
    let slot = TakeSlot::new(Rc::clone(&value));
    slot.try_resolve_ref().unwrap().set(9);
    assert_eq!(value.get(), 9);
    assert_eq!(slot.try_resolve().unwrap().get(), 9);
}

#[test]
fn value_is_dropped_exactly_once_after_take_or_slot_drop() {
    struct CountDrop(Arc<AtomicUsize>);
    impl Drop for CountDrop {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }
    let drops = Arc::new(AtomicUsize::new(0));
    let slot = TakeSlot::new(CountDrop(Arc::clone(&drops)));
    let value = slot.try_resolve().unwrap();
    assert_consumed(slot.try_resolve());
    drop(slot);
    assert_eq!(drops.load(Ordering::SeqCst), 0);
    drop(value);
    assert_eq!(drops.load(Ordering::SeqCst), 1);
    drop(TakeSlot::new(CountDrop(Arc::clone(&drops))));
    assert_eq!(drops.load(Ordering::SeqCst), 2);
}

#[test]
fn explicit_clone_is_observable_even_for_copy_values() {
    #[derive(Copy)]
    struct CountClone<'a>(&'a AtomicUsize);
    #[allow(clippy::non_canonical_clone_impl)]
    impl Clone for CountClone<'_> {
        fn clone(&self) -> Self {
            self.0.fetch_add(1, Ordering::SeqCst);
            *self
        }
    }
    let clones = AtomicUsize::new(0);
    let slot = CopySlot::new(CountClone(&clones));
    let _copy = slot.resolve();
    let _reference = slot.resolve_ref();
    assert_eq!(clones.load(Ordering::SeqCst), 0);
    let Ok(_copy) = slot.try_resolve();
    assert_eq!(clones.load(Ordering::SeqCst), 0);
    let clone_slot = systasis::__private::ReadSlot::new(CountClone(&clones));
    let _clone = clone_slot.resolve_clone();
    assert_eq!(clones.load(Ordering::SeqCst), 1);
    let slot = TakeSlot::new(CountClone(&clones));
    let _clone = slot.try_resolve_clone().unwrap();
    assert_eq!(clones.load(Ordering::SeqCst), 2);
    let guard = slot.try_resolve_ref_mut().unwrap();
    assert_contended(slot.try_resolve_clone());
    assert_eq!(clones.load(Ordering::SeqCst), 2);
    drop(guard);
    let _original = slot.try_resolve().unwrap();
    assert_consumed(slot.try_resolve_clone());
    assert_eq!(clones.load(Ordering::SeqCst), 2);
}

#[test]
fn barrier_establishes_cross_thread_contention_without_sleeps() {
    let slot = TakeSlot::new(7_u32);
    let acquired = Barrier::new(2);
    let checked = Barrier::new(2);
    std::thread::scope(|scope| {
        scope.spawn(|| {
            let guard = slot.try_resolve_ref_mut().unwrap();
            acquired.wait();
            checked.wait();
            assert_eq!(*guard, 7);
        });
        acquired.wait();
        assert_contended(slot.try_resolve());
        assert_contended(slot.try_resolve_ref());
        checked.wait();
    });
    assert_eq!(slot.try_resolve().unwrap(), 7);
}

#[test]
fn guard_references_can_be_shared_when_value_is_sync() {
    fn assert_sync<T: Sync>() {}
    assert_sync::<Ref<'_, u32>>();
    assert_sync::<RefMut<'_, u32>>();
    let slot = TakeSlot::new(7_u32);
    let guard = slot.try_resolve_ref_mut().unwrap();
    std::thread::scope(|scope| {
        scope.spawn(|| assert_eq!(*guard, 7)).join().unwrap();
    });
    drop(guard);
    assert_eq!(slot.try_resolve().unwrap(), 7);
}

#[test]
fn forgotten_guard_preserves_exclusion_without_invalidating_slot_drop() {
    let slot = TakeSlot::new(String::from("value"));
    std::mem::forget(slot.try_resolve_ref().unwrap());
    assert_contended(slot.try_resolve());
    assert_contended(slot.try_resolve_ref_mut());
    assert_eq!(&*slot.try_resolve_ref().unwrap(), "value");
    drop(slot);
}

#[test]
fn caller_write_panic_releases_lock_without_poisoning() {
    let slot = TakeSlot::new(String::from("value"));
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut guard = slot.try_resolve_ref_mut().unwrap();
            guard.push('!');
            panic!("caller failure");
        }))
        .is_err()
    );
    assert_eq!(slot.try_resolve().unwrap(), "value!");
}

#[test]
fn caller_read_panic_releases_lock() {
    let slot = TakeSlot::new(7_u32);
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = slot.try_resolve_ref().unwrap();
            panic!("caller failure");
        }))
        .is_err()
    );
    assert_eq!(slot.try_resolve().unwrap(), 7);
}

#[test]
fn guards_can_be_transferred_and_dropped_on_another_thread() {
    let slot = TakeSlot::new(String::from("value"));
    std::thread::scope(|scope| {
        let reader = slot.try_resolve_ref().unwrap();
        scope
            .spawn(move || assert_eq!(&*reader, "value"))
            .join()
            .unwrap();
        let projected = Ref::map(slot.try_resolve_ref().unwrap(), String::as_str);
        scope
            .spawn(move || assert_eq!(&*projected, "value"))
            .join()
            .unwrap();
        let mut writer = slot.try_resolve_ref_mut().unwrap();
        scope.spawn(move || writer.push('!')).join().unwrap();
    });
    assert_eq!(slot.try_resolve().unwrap(), "value!");
}

#[test]
fn caller_projection_panic_releases_read_lock() {
    let slot = TakeSlot::new(7_u32);
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _: Ref<'_, u32> = Ref::map(slot.try_resolve_ref().unwrap(), |_| {
                panic!("caller failure")
            });
        }))
        .is_err()
    );
    assert_eq!(slot.try_resolve().unwrap(), 7);
}
