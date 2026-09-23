use super::{assert_no_allocations, measure};
use core::sync::atomic::{AtomicBool, Ordering};

static FAIL: AtomicBool = AtomicBool::new(false);
trait IFlag {}
impl IFlag for bool {}
systasis::systasis_container! {
    register_value!("42".parse::<u8>()?: u8 as Copy);
    register_value!((if FAIL.load(Ordering::Relaxed) { "bad" } else { "true" }).parse::<bool>()?: bool as IFlag);
}

#[test]
fn building_and_returning_inferred_errors_do_not_allocate() {
    let (number, counts) = measure(|| {
        let container = SystasisContainer::build().unwrap();
        std::hint::black_box(container.resolve_copy())
    });
    assert_eq!(number, 42);
    assert_no_allocations(counts);
    FAIL.store(true, Ordering::Relaxed);
    let ((), counts) = measure(|| {
        let Err(error) = SystasisContainer::build() else {
            panic!("selected input must fail")
        };
        assert!(
            core::error::Error::source(&error)
                .unwrap()
                .is::<core::str::ParseBoolError>()
        );
        drop(std::hint::black_box(error));
    });
    assert_no_allocations(counts);
}
