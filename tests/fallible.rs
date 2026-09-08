//! Full Option/Result conversion contract; no extra trait bounds on payloads.
use std::{cell::Cell, rc::Rc};
use systasis::Fallible;

#[test]
fn option_preserves_value_or_reports_unit() {
    assert_eq!(Some(3).into_result(), Ok(3));
    assert_eq!(None::<u8>.into_result(), Err(()));
    assert_eq!(Some(3).into_option(), Some(3));
    assert_eq!(None::<u8>.into_option(), None);
}

#[test]
fn result_preserves_value_and_error_exactly() {
    let error = Rc::new(Cell::new(7));
    let result: Result<(), _> = Err(Rc::clone(&error));
    let returned = result.into_result().expect_err("preserves Err");
    assert!(Rc::ptr_eq(&error, &returned));
    assert_eq!(Ok::<_, ()>(4).into_result(), Ok(4));
    assert_eq!(Ok::<_, ()>(5).into_option(), Some(5));
}

#[test]
fn discarding_error_drops_it_once() {
    struct Tracked<'a>(&'a Cell<usize>);
    impl Drop for Tracked<'_> {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }
    let drops = Cell::new(0);
    assert!(Err::<(), _>(Tracked(&drops)).into_option().is_none());
    assert_eq!(drops.get(), 1);
}
