#![no_std]
#![forbid(unsafe_code)]

use systasis::{Fallible, container::Error};

/// Positive control for the generated path in no_std target compilation.
pub mod generated {
    pub trait IValue {}
    impl IValue for u32 {}

    #[systasis::container]
    pub fn round_trip(input: u32) -> u32 {
        let Ok(container) = systasis::systasis_container! {
            register_value!(input: u32 as IValue);
        }.build();
        container.resolve_i_value()
    }
}

/// Generic code generation is also compiled by the embedded-target checks.
pub mod generic_generated {
    #[systasis::container]
    pub fn round_trip<T: Copy + crate::generated::IValue>(input: T) -> T {
        let Ok(container) = systasis::systasis_container! {
            register_value!(input: T as crate::generated::IValue);
        }.build();
        container.resolve_i_value()
    }
}

#[cfg(feature = "bad-local-sync")]
pub fn local_slot_cannot_be_shared() {
    fn sync<T: Sync>() {}
    sync::<systasis::__private::LocalTakeSlot<u32>>();
}

#[cfg(feature = "bad-local-guard-send")]
pub fn local_guard_cannot_cross_threads() {
    fn send<T: Send>(_: T) {}
    let slot = systasis::__private::LocalTakeSlot::new(7_u32);
    send(slot.try_resolve_ref().unwrap());
}

#[cfg(feature = "bad-readonly-take")]
pub fn readonly_value_cannot_be_taken() {
    let slot = systasis::__private::ReadSlot::new(7_u32);
    slot.try_resolve();
}

extern crate alloc;

/// Positive control: reservation adds no independent lifetime or guard type.
pub fn reserved_reference(
    slot: &systasis::__private::TakeSlot<u32>,
) -> Result<&u32, Error> {
    slot.try_reserve_ref()
}

/// Positive controls for unchanged field-derived thread-safety contracts.
pub fn slot_and_guard_traits() {
    fn send<T: Send>() {}
    fn sync<T: Sync>() {}
    send::<systasis::__private::TakeSlot<core::cell::Cell<u32>>>();
    sync::<systasis::__private::TakeSlot<u32>>();
    sync::<systasis::Ref<'static, u32>>();
    sync::<systasis::RefMut<'static, u32>>();
    {
        send::<systasis::Ref<'static, u32>>();
        send::<systasis::RefMut<'static, core::cell::Cell<u32>>>();
    }
}

pub fn error() -> Error {
    Error::ValueAlreadyConsumed
}

pub fn convert(value: Option<u8>) -> Result<u8, ()> {
    value.into_result()
}

#[cfg(feature = "error-value-traits")]
pub fn error_supports_value_traits() {
    fn requires_value_traits<T: Clone + Copy + core::fmt::Debug + Eq + PartialEq>() {}
    requires_value_traits::<Error>();
}

#[cfg(feature = "bad-poison-in-no-std")]
pub fn poison_requires_std(error: Error) -> bool {
    matches!(error, Error::PoisonedLock(_))
}

#[cfg(feature = "bad-fallible")]
pub fn ordinary_value_is_not_fallible() {
    fn requires_fallible<T: Fallible>() {}
    requires_fallible::<u32>();
}

#[cfg(feature = "bad-guard-escape")]
pub fn guard_cannot_outlive_slot() -> systasis::Ref<'static, u32> {
    let slot = systasis::__private::TakeSlot::new(7);
    slot.try_resolve_ref().unwrap()
}

#[cfg(feature = "bad-sync-cell-guard")]
pub fn non_sync_payload_cannot_be_shared() {
    fn requires_sync<T: Sync>() {}
    requires_sync::<systasis::Ref<'static, core::cell::Cell<u32>>>();
}

#[cfg(feature = "bad-reserved-escape")]
pub fn reserved_reference_cannot_outlive_slot() -> &'static u32 {
    let slot = systasis::__private::TakeSlot::new(7);
    slot.try_reserve_ref().unwrap()
}

#[cfg(feature = "bad-move-reserved-slot")]
pub fn live_reservation_prevents_slot_move() -> u32 {
    let slot = systasis::__private::TakeSlot::new(7);
    let reserved = slot.try_reserve_ref().unwrap();
    drop(slot);
    *reserved
}

#[cfg(feature = "bad-sync-cell-slot")]
pub fn reservation_does_not_make_payload_sync() {
    fn requires_sync<T: Sync>() {}
    requires_sync::<systasis::__private::TakeSlot<core::cell::Cell<u32>>>();
}

#[cfg(feature = "bad-send-rc-slot")]
pub fn reservation_does_not_make_payload_send() {
    fn requires_send<T: Send>() {}
    requires_send::<systasis::__private::TakeSlot<alloc::rc::Rc<u32>>>();
}

#[cfg(feature = "bad-send-cell-reader")]
pub fn shared_guard_of_non_sync_payload_cannot_be_sent() {
    fn requires_send<T: Send>() {}
    requires_send::<systasis::Ref<'static, core::cell::Cell<u32>>>();
}

#[cfg(feature = "bad-mutable-lifetime")]
pub fn mutable_guard_cannot_store_short_reference(
    slot: &systasis::__private::TakeSlot<&'static str>,
    value: &str,
) {
    *slot.try_resolve_ref_mut().unwrap() = value;
}
