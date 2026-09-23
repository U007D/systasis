//! Lock-free storage policies preserve occupancy and borrowing checks.
#![forbid(unsafe_code)]
use systasis::{
    __private::{LocalTakeSlot, ReadSlot},
    container::Error,
};

#[test]
fn read_only_storage_is_plain_and_can_be_shared() {
    fn sync<T: Sync>() {}
    sync::<ReadSlot<String>>();
    assert_eq!(size_of::<ReadSlot<String>>(), size_of::<String>());
    let slot = ReadSlot::new(String::from("value"));
    let first = slot.resolve_ref();
    assert!(core::ptr::eq(first, slot.resolve_ref()));
    assert_eq!(slot.resolve_clone(), "value");
}

#[test]
fn local_borrows_exclude_conflicting_access_and_release_on_drop() {
    let slot = LocalTakeSlot::new(String::from("value"));
    let reader = slot.try_resolve_ref().unwrap();
    assert_eq!(&*slot.try_resolve_ref().unwrap(), "value");
    assert!(matches!(
        slot.try_resolve(),
        Err(Error::ValueAccessContention)
    ));
    assert!(matches!(
        slot.try_resolve_ref_mut(),
        Err(Error::ValueAccessContention)
    ));
    assert_eq!(slot.try_resolve_clone().unwrap(), "value");
    drop(reader);
    let mut writer = slot.try_resolve_ref_mut().unwrap();
    writer.push('!');
    assert!(matches!(
        slot.try_resolve_ref(),
        Err(Error::ValueAccessContention)
    ));
    drop(writer);
    assert_eq!(slot.try_resolve().unwrap(), "value!");
    assert!(matches!(
        slot.try_resolve_ref(),
        Err(Error::ValueAlreadyConsumed)
    ));
    assert!(matches!(
        slot.try_resolve_ref_mut(),
        Err(Error::ValueAlreadyConsumed)
    ));
    assert!(matches!(
        slot.try_resolve_clone(),
        Err(Error::ValueAlreadyConsumed)
    ));
    assert!(matches!(
        slot.try_resolve(),
        Err(Error::ValueAlreadyConsumed)
    ));
}

#[test]
fn local_slot_is_send_when_payload_is_send_and_accepts_non_send_values() {
    fn send<T: Send>() {}
    send::<LocalTakeSlot<core::cell::Cell<u32>>>();
    let value = std::rc::Rc::new(7);
    let slot = LocalTakeSlot::new(value.clone());
    assert!(std::rc::Rc::ptr_eq(&slot.try_resolve().unwrap(), &value));
}
