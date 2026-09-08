#![no_std]
#![forbid(unsafe_code)]

use systasis::{Fallible, app_container::Error};

pub fn error() -> Error {
    Error::ValueAlreadyConsumed
}

pub fn convert(value: Option<u8>) -> Result<u8, ()> {
    value.into_result()
}

#[cfg(feature = "bad-copy-error")]
pub fn error_is_not_copy() {
    fn requires_copy<T: Copy>() {}
    requires_copy::<Error>();
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

#[cfg(feature = "bad-send-guard")]
pub fn std_guard_cannot_move_between_threads() {
    fn requires_send<T: Send>() {}
    requires_send::<systasis::Ref<'static, u32>>();
}

#[cfg(feature = "bad-send-mut-guard")]
pub fn std_mut_guard_cannot_move_between_threads() {
    fn requires_send<T: Send>() {}
    requires_send::<systasis::RefMut<'static, u32>>();
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
