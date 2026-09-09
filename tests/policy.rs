//! Compile-time policy dispatch must not add synchronization to Copy values.
#![forbid(unsafe_code)]

use systasis::__private::{CopyFallback, CopySlot, LocalTakeSlot, Pick, Policy, Select, TakeSlot};

#[derive(Clone, Copy)]
struct Number(u32);

type Alias = Number;
const COPY: bool = Pick::<Alias>::IS_COPY;
const OWNED: bool = Pick::<String>::IS_COPY;

#[test]
fn copy_selection_is_plain_in_both_modes_including_aliases() {
    let local: CopySlot<Number> = <Policy<COPY, true> as Select<_>>::store(Number(7));
    let shared: CopySlot<Number> = <Policy<COPY, false> as Select<_>>::store(Number(9));
    assert_eq!(local.resolve().0, 7);
    assert_eq!(shared.resolve().0, 9);
    assert_eq!(size_of_val(&local), size_of::<Number>());
    assert_eq!(size_of_val(&shared), size_of::<Number>());
}

#[test]
fn non_copy_selection_uses_the_requested_policy() {
    let local: LocalTakeSlot<String> =
        <Policy<OWNED, true> as Select<_>>::store(String::from("local"));
    let shared: TakeSlot<String> =
        <Policy<OWNED, false> as Select<_>>::store(String::from("shared"));
    assert_eq!(local.try_resolve().unwrap(), "local");
    assert_eq!(shared.try_resolve().unwrap(), "shared");
}

#[test]
fn generic_probe_does_not_change_at_monomorphization() {
    fn unbounded<T>() -> bool {
        Pick::<T>::IS_COPY
    }
    fn bounded<T: Copy>() -> bool {
        Pick::<T>::IS_COPY
    }
    assert!(!unbounded::<u32>());
    assert!(bounded::<u32>());
}

#[test]
fn generic_evidence_uses_bounds_instead_of_monomorphized_type() {
    use systasis::__private::{CopyKnown, CopyUnknown, DetectCopy, verify_generic_fallback};
    #[allow(clippy::needless_borrow)] // Both receiver levels are intentional for autoref selection.
    fn unbounded<T>() -> CopyUnknown {
        let evidence = (&&Pick::<T>::NEW).evidence();
        verify_generic_fallback(evidence);
        (&&Pick::<T>::NEW).evidence()
    }
    fn bounded<T: Copy>() -> CopyKnown {
        (&&Pick::<T>::NEW).evidence()
    }
    let _: CopyUnknown = unbounded::<u32>();
    let _: CopyUnknown = unbounded::<String>();
    let _: CopyKnown = bounded::<u32>();
}
