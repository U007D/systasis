//! A builder owns pending work; only building executes stored initializers.
#![forbid(unsafe_code)]

use std::{cell::Cell, rc::Rc};

struct Tracked(Rc<Cell<usize>>);
impl Drop for Tracked {
    fn drop(&mut self) {
        self.0.set(self.0.get() + 1);
    }
}
trait IValue {}
impl IValue for Tracked {}
impl IValue for usize {}
impl IValue for String {}
trait IFresh {}
impl IFresh for usize {}

mod abandoned {
    use super::*;

    #[systasis::container]
    #[test]
    fn dropping_builder_skips_initializers_and_drops_owned_inputs() {
        let drops: Rc<Cell<usize>> = Rc::new(Cell::new(0));
        let calls: Cell<usize> = Cell::new(0);
        let value: Tracked = Tracked(Rc::clone(&drops));
        let builder = systasis::systasis_container! {
            register_value!({ calls.set(calls.get() + 1); value }: Tracked as IValue);
        };
        assert_eq!(calls.get(), 0);
        assert_eq!(drops.get(), 0);
        drop(builder);
        assert_eq!(calls.get(), 0);
        assert_eq!(drops.get(), 1);
    }
}

mod captured {
    use super::*;

    #[systasis::container]
    #[test]
    fn abandoning_builder_drops_fresh_constructor_captures_without_calling_it() {
        let drops: Rc<Cell<usize>> = Rc::new(Cell::new(0));
        let value: Tracked = Tracked(Rc::clone(&drops));
        let builder = systasis::systasis_container! {
            register_type_with!(usize as IFresh, move || value.0.get());
        };
        assert_eq!(drops.get(), 0);
        drop(builder);
        assert_eq!(drops.get(), 1);
    }
}

mod delayed {
    use super::*;

    fn read(container: &AppContainer) -> usize {
        container.resolve_i_value()
    }

    #[systasis::container]
    #[test]
    fn build_runs_once_and_publishes_the_nameable_container() {
        let calls: Cell<usize> = Cell::new(0);
        let text: String = String::from("captured");
        let builder = systasis::systasis_container! {
            register_value!({ calls.set(calls.get() + 1); 7_usize }: usize as IValue);
            register_type_with!(usize as IFresh, move || text.len() + resolve!(IValue));
        };
        assert_eq!(calls.get(), 0);
        let text: String = String::from("a different binding");
        let moved_builder = builder;
        let Ok(container) = moved_builder.build();
        assert_eq!(calls.get(), 1);
        assert_eq!(read(container), 7);
        assert_eq!(container.resolve_i_fresh(), 15);
        assert_eq!(container.resolve_i_fresh(), 15);
        assert_eq!(text, "a different binding");
    }
}

mod borrowed_input {
    use super::*;

    #[systasis::container]
    #[test]
    #[allow(
        clippy::drop_non_drop,
        reason = "explicit abandonment must end the initializer's exclusive input borrow"
    )]
    fn abandonment_releases_a_build_only_mutable_borrow() {
        let mut input: String = String::from("before");
        let builder = systasis::systasis_container! {
            register_value!({ input.push('!'); input.len() }: usize as IValue);
        };
        drop(builder);
        input.push('?');
        assert_eq!(input, "before?");
    }
}

mod built_borrow {
    use super::*;

    #[systasis::container]
    #[test]
    fn building_releases_a_build_only_mutable_borrow() {
        let mut input: String = String::from("before");
        let builder = systasis::systasis_container! {
            register_value!({ input.push('!'); input.len() }: usize as IValue);
        };
        let Ok(container) = builder.build::<_>();
        input.push('?');
        assert_eq!(input, "before!?");
        assert_eq!(container.resolve_i_value(), 7);
    }
}

mod overrides {
    use super::*;

    #[systasis::container]
    #[test]
    fn discarded_registrations_do_not_capture_owned_inputs() {
        let loser: String = String::from("not captured");
        let winner: String = String::from("selected");
        let builder = systasis::systasis_container! {
            register_type_with!(usize as IFresh, move || loser.len());
            register_type_with!(usize as IFresh, move || winner.len());
        };
        drop(builder);
        assert_eq!(loser, "not captured");
    }
}

mod failed {
    use super::*;

    #[derive(Debug, PartialEq)]
    struct Rejected;

    #[systasis::container]
    #[test]
    fn delayed_fallible_build_cleans_up_before_returning() {
        let drops: Rc<Cell<usize>> = Rc::new(Cell::new(0));
        let value: Tracked = Tracked(Rc::clone(&drops));
        let builder = systasis::systasis_container! {
            register_value!(value: Tracked as IValue);
            register_value!({
                let _guard = try_resolve_ref!(IValue).map_err(|_| Rejected)?;
                Err::<usize, Rejected>(Rejected)?
            }: usize as IFresh);
        };
        assert_eq!(drops.get(), 0);
        let built = builder.build::<Rejected>();
        assert!(matches!(built, Err(Rejected)));
        assert_eq!(drops.get(), 1);
    }
}
