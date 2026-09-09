//! Permanent read reservations preserve ordinary references without held locks.
#![forbid(unsafe_code)]

use std::{
    cell::Cell,
    sync::{
        Barrier,
        atomic::{AtomicUsize, Ordering},
    },
};
use systasis::{__private::TakeSlot, Ref, app_container::Error};

fn assert_contended<T>(result: Result<T, Error>) {
    assert!(matches!(result, Err(Error::ValueAccessContention)));
}

#[test]
fn reservation_rejects_mutation_and_take_before_touching_the_payload() {
    let slot = TakeSlot::new(String::from("original"));
    let reserved = slot.try_reserve_ref().unwrap();
    for _ in 0..3 {
        assert_contended(slot.try_resolve_ref_mut());
        assert_contended(slot.try_resolve());
        assert_eq!(reserved, "original");
        assert_eq!(&*slot.try_resolve_ref().unwrap(), reserved);
        assert_eq!(slot.try_resolve_clone().unwrap(), *reserved);
    }
}

#[test]
fn reservation_survives_the_last_reference_use_and_dependent_drop() {
    struct Service<'a>(&'a String);
    impl Drop for Service<'_> {
        fn drop(&mut self) {
            assert_eq!(self.0, "dependency");
        }
    }
    let slot = TakeSlot::new(String::from("dependency"));
    let service = Service(slot.try_reserve_ref().unwrap());
    drop(service);
    assert_contended(slot.try_resolve());
    assert_contended(slot.try_resolve_ref_mut());
    assert_eq!(slot.try_reserve_ref().unwrap(), "dependency");
}

#[test]
fn repeated_reservation_releases_its_lock_and_keeps_the_same_reference() {
    let slot = TakeSlot::new(String::from("value"));
    let first = slot.try_reserve_ref().unwrap();
    let second = slot.try_reserve_ref().unwrap();
    assert!(std::ptr::eq(first, second));
    let projected = Ref::map(slot.try_resolve_ref().unwrap(), String::as_str);
    let projected = Ref::map(projected, str::as_bytes);
    assert_eq!(&*projected, b"value");
    // Reservation acquisition always needs a write lock, even if reserved.
    assert_contended(slot.try_reserve_ref());
    drop(projected);
    assert!(std::ptr::eq(first, slot.try_reserve_ref().unwrap()));
}

#[test]
fn failed_reservation_does_not_freeze_an_ordinary_borrowed_slot() {
    let slot = TakeSlot::new(String::from("value"));
    let reader = slot.try_resolve_ref().unwrap();
    assert_contended(slot.try_reserve_ref());
    drop(reader);
    let mut writer = slot.try_resolve_ref_mut().unwrap();
    writer.push('!');
    assert_contended(slot.try_reserve_ref());
    drop(writer);
    assert_eq!(slot.try_resolve().unwrap(), "value!");
}

#[test]
fn consumed_reservation_reports_consumption_and_releases_its_lock() {
    let slot = TakeSlot::new(String::from("value"));
    drop(slot.try_resolve().unwrap());
    for _ in 0..3 {
        assert!(matches!(
            slot.try_reserve_ref(),
            Err(Error::ValueAlreadyConsumed)
        ));
        assert!(matches!(
            slot.try_resolve_ref_mut(),
            Err(Error::ValueAlreadyConsumed)
        ));
    }
}

#[test]
fn reserved_reference_is_shareable_without_retaining_a_non_send_guard() {
    let slot = TakeSlot::new(String::from("value"));
    let reserved = slot.try_reserve_ref().unwrap();
    let checked = Barrier::new(2);
    std::thread::scope(|scope| {
        scope.spawn(|| {
            assert_contended(slot.try_resolve());
            assert_contended(slot.try_resolve_ref_mut());
            assert_eq!(slot.try_resolve_clone().unwrap(), *reserved);
            checked.wait();
        });
        checked.wait();
        assert_eq!(reserved, "value");
    });
}

#[test]
fn reservation_does_not_require_sync_for_local_values() {
    let slot = TakeSlot::new(Cell::new(7));
    let reserved = slot.try_reserve_ref().unwrap();
    reserved.set(9);
    assert_eq!(slot.try_resolve_ref().unwrap().get(), 9);
    assert_contended(slot.try_resolve_ref_mut());
    assert_eq!(reserved.get(), 9);
}

#[test]
fn reserved_payload_is_dropped_once_even_with_a_forgotten_reader() {
    struct CountDrop<'a>(&'a AtomicUsize);
    impl Drop for CountDrop<'_> {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }
    let drops = AtomicUsize::new(0);
    let slot = TakeSlot::new(CountDrop(&drops));
    assert_eq!(slot.try_reserve_ref().unwrap().0.load(Ordering::SeqCst), 0);
    std::mem::forget(slot.try_resolve_ref().unwrap());
    assert_contended(slot.try_reserve_ref());
    drop(slot);
    assert_eq!(drops.load(Ordering::SeqCst), 1);
}

#[test]
fn zero_sized_and_overaligned_reservations_remain_valid() {
    #[repr(align(256))]
    struct Aligned(u32);
    let zero = TakeSlot::new(());
    let reserved = zero.try_reserve_ref().unwrap();
    assert_contended(zero.try_resolve_ref_mut());
    assert_eq!(*reserved, ());

    let aligned = TakeSlot::new(Aligned(7));
    let reserved = aligned.try_reserve_ref().unwrap();
    assert_eq!((reserved as *const Aligned).addr() % 256, 0);
    assert_contended(aligned.try_resolve());
    assert_eq!(reserved.0, 7);
}
