//! Measure construction, deferred constructors, failure cleanup, and destruction.
use super::{assert_no_allocations, measure};
use core::cell::Cell;

struct Tracked<'a>(&'a Cell<usize>);
impl Drop for Tracked<'_> {
    fn drop(&mut self) {
        self.0.set(self.0.get() + 1);
    }
}
trait ITracked {}
impl ITracked for Tracked<'_> {}
trait IFailing {}
impl IFailing for u32 {}
trait IFresh {}
impl IFresh for [u8; 16] {}

mod success {
    use super::*;
    #[systasis::container]
    fn build_and_drop(drops: &Cell<usize>) {
        let Ok(container) = systasis::systasis_container! {
            register_value!(Tracked(drops): Tracked<'_> as ITracked);
        }
        .build();
        assert_eq!(drops.get(), 0);
        drop(container.try_resolve_i_tracked_ref().unwrap());
    }
    #[test]
    fn successful_owner_destruction_does_not_allocate() {
        let drops = Cell::new(0);
        let (_, counts) = measure(|| build_and_drop(&drops));
        assert_no_allocations(counts);
        assert_eq!(drops.get(), 1);
    }
}

mod failure {
    use super::*;
    #[derive(Debug, PartialEq)]
    struct Failure;
    #[systasis::container]
    fn fail_and_cleanup(initialized: &Cell<usize>, captures: &Cell<usize>) {
        let capture: Tracked<'_> = Tracked(captures);
        let built = systasis::systasis_container! {
            register_value!(Tracked(initialized): Tracked<'_> as ITracked);
            register_value!(Err::<u32, Failure>(Failure)?: u32 as IFailing);
            register_type_with!([u8; 16] as IFresh, move || {
                let _ = &capture;
                [0; 16]
            });
        }
        .build::<Failure>();
        assert!(matches!(built, Err(Failure)));
        assert_eq!(initialized.get(), 1);
        assert_eq!(captures.get(), 1);
    }
    #[test]
    fn failed_build_immediately_drops_prior_state_and_unused_captures_without_allocation() {
        let initialized = Cell::new(0);
        let captures = Cell::new(0);
        let (_, counts) = measure(|| fail_and_cleanup(&initialized, &captures));
        assert_no_allocations(counts);
        assert_eq!(initialized.get(), 1);
        assert_eq!(captures.get(), 1);
    }
}

mod fresh {
    use super::*;
    trait IDefault {}
    impl IDefault for [u8; 16] {}
    #[systasis::container]
    fn constructors<'a>(calls: &'a Cell<usize>, drops: &Cell<usize>) {
        let capture: Tracked<'_> = Tracked(drops);
        let Ok(container) = systasis::systasis_container! {
            register_type!([u8; 16] as IDefault);
            register_type_with!([u8; 16] as IFresh, move || {
                let _ = &capture;
                calls.set(calls.get() + 1);
                [7; 16]
            });
        }
        .build();
        assert_eq!(calls.get(), 0);
        assert_eq!(container.resolve_i_default(), [0; 16]);
        assert_eq!(container.resolve_i_fresh(), [7; 16]);
        assert_eq!(container.resolve_i_fresh(), [7; 16]);
        assert_eq!(calls.get(), 2);
        assert_eq!(drops.get(), 0);
    }
    #[test]
    fn cold_and_repeated_constructors_and_capture_cleanup_do_not_allocate() {
        let calls = Cell::new(0);
        let drops = Cell::new(0);
        let (_, counts) = measure(|| constructors(&calls, &drops));
        assert_no_allocations(counts);
        assert_eq!(calls.get(), 2);
        assert_eq!(drops.get(), 1);
    }
}

mod returned_guard {
    use super::*;
    struct Value([u8; 16]);
    trait IValue {}
    impl IValue for Value {}
    trait IGuard {}
    impl IGuard for systasis::Ref<'_, Value> {}
    #[systasis::container]
    fn guards() -> Result<(), systasis::app_container::Error> {
        let container = systasis::systasis_container! {
            register_value!(Value([9; 16]): Value as IValue);
            register_type_with!(systasis::Ref<'_, Value> as IGuard, try || -> Result<systasis::Ref<'_, Value>, systasis::app_container::Error> {
                try_resolve_ref!(IValue)
            });
        }.build::<systasis::app_container::Error>()?;
        {
            let guard = container.try_resolve_i_guard()?;
            assert_eq!(guard.0, [9; 16]);
            assert!(matches!(
                container.try_resolve_i_value_ref_mut(),
                Err(systasis::app_container::Error::ValueAccessContention)
            ));
        }
        container.try_resolve_i_value_ref_mut()?.0[0] = 3;
        Ok(())
    }
    #[test]
    fn returned_constructor_guards_do_not_allocate() {
        let (result, counts) = measure(guards);
        assert_no_allocations(counts);
        result.unwrap();
    }
}
