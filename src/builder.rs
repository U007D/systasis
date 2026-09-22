//! Deferred initialization that transfers the completed container to its caller.

use core::marker::PhantomData;

/// A consuming initializer for an owned generated container.
///
/// The initializer retains pending captures and borrows until build or drop.
/// Its output type imposes no auto-trait requirements on the pending work.
pub struct Builder<T, F> {
    initialize: F,
    output: PhantomData<fn() -> T>,
}

impl<T, F> Builder<T, F> {
    /// Retain initialization without executing it.
    pub const fn new<E>(initialize: F) -> Self
    where
        F: FnOnce() -> Result<T, E>,
    {
        Self {
            initialize,
            output: PhantomData,
        }
    }

    /// Initialize once and return ownership of the successful value.
    ///
    /// Initializer-local state and remaining captured inputs are dropped
    /// normally; values moved into the result remain owned by that result.
    pub fn build<E>(self) -> Result<T, E>
    where
        F: FnOnce() -> Result<T, E>,
    {
        (self.initialize)()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::{convert::Infallible, marker::PhantomPinned, pin::pin};

    struct Tracked<'a>(&'a core::cell::Cell<usize>);

    impl Drop for Tracked<'_> {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }

    #[derive(Debug, PartialEq)]
    struct Failure(u8);

    #[test]
    fn initialization_waits_for_build_and_publishes_the_owned_value() {
        let calls = core::cell::Cell::new(0);
        let builder = Builder::new(|| {
            calls.set(calls.get() + 1);
            Ok::<_, Infallible>(17)
        });

        assert_eq!(calls.get(), 0);
        let value = builder.build().unwrap();
        assert_eq!(value, 17);
        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn abandoning_builder_drops_owned_captures_without_initializing() {
        let drops = core::cell::Cell::new(0);
        let capture = Tracked(&drops);
        let builder = Builder::new(move || Ok::<_, Infallible>(capture));

        assert_eq!(drops.get(), 0);
        drop(builder);
        assert_eq!(drops.get(), 1);
    }

    #[test]
    #[allow(
        clippy::drop_non_drop,
        reason = "explicit abandonment must end the initializer's exclusive input borrow"
    )]
    fn abandoning_builder_releases_mutably_borrowed_inputs() {
        let mut input = 3;
        let builder = Builder::new(|| {
            input += 1;
            Ok::<_, Infallible>(input)
        });

        drop(builder);
        input += 2;
        assert_eq!(input, 5, "the abandoned initializer did not run");
    }

    #[test]
    fn building_releases_temporary_input_borrows_while_result_remains_live() {
        let mut input = 3;
        let builder = Builder::new(|| {
            input += 1;
            Ok::<_, Infallible>(input)
        });

        let value = builder.build().unwrap();
        input += 2;
        assert_eq!((value, input), (4, 6));
    }

    #[test]
    fn failure_preserves_its_type_and_cleans_up_partial_state_and_captures() {
        let capture_drops = core::cell::Cell::new(0);
        let partial_drops = core::cell::Cell::new(0);
        let capture = Tracked(&capture_drops);
        let partial_counter = &partial_drops;
        let builder = Builder::new(move || {
            let _partial = Tracked(partial_counter);
            let _ = &capture;
            Err(Failure(7))
        });

        let result: Result<u8, Failure> = builder.build();
        assert_eq!(result, Err(Failure(7)));
        assert_eq!(capture_drops.get(), 1);
        assert_eq!(partial_drops.get(), 1);
    }

    #[test]
    fn failure_releases_temporary_mutable_input_borrows() {
        let mut input = 3;
        let builder = Builder::new(|| {
            input += 1;
            Err(Failure(7))
        });

        let result: Result<u8, Failure> = builder.build();
        input += 2;
        assert_eq!(result, Err(Failure(7)));
        assert_eq!(input, 6, "initializer side effects are not rolled back");
    }

    #[test]
    fn an_error_keeps_ownership_of_values_moved_into_it() {
        let drops = core::cell::Cell::new(0);
        let capture = Tracked(&drops);
        let builder = Builder::new(move || Err::<(), _>(capture));

        let Err(error) = builder.build() else {
            panic!("the initializer always returns its capture as an error");
        };
        assert_eq!(drops.get(), 0);
        drop(error);
        assert_eq!(drops.get(), 1);
    }

    #[test]
    fn owned_non_unpin_value_can_be_pinned_by_its_caller() {
        struct Value {
            number: u8,
            _pin: PhantomPinned,
        }

        let builder = Builder::new(|| {
            Ok::<_, Infallible>(Value {
                number: 7,
                _pin: PhantomPinned,
            })
        });
        let value = builder.build().unwrap();
        let pinned = pin!(value);
        assert_eq!(pinned.as_ref().get_ref().number, 7);
    }

    fn error_type<T, E>(_: &Result<T, E>) -> &'static str {
        core::any::type_name::<E>()
    }

    #[test]
    fn empty_match_preserves_unconstrained_never_error_inference() {
        let builder = Builder::new(|| Ok::<_, Infallible>(7_u8).map_err(|never| match never {}));
        let result = builder.build();
        assert_eq!(error_type(&result), "!");
        let Ok(value) = result;
        assert_eq!(value, 7);
    }

    #[test]
    fn placeholder_build_error_preserves_never_fallback() {
        let builder = Builder::new(|| Ok::<_, Infallible>(7_u8).map_err(|never| match never {}));
        let result = builder.build::<_>();
        assert_eq!(error_type(&result), "!");
    }

    #[test]
    fn explicit_build_error_type_supplies_initializer_context() {
        let builder = Builder::new(|| Ok::<_, Infallible>(7_u8).map_err(|never| match never {}));
        let result: Result<u8, Failure> = builder.build::<Failure>();
        assert_eq!(result, Ok(7));
    }

    #[test]
    fn surrounding_result_type_supplies_initializer_error_context() {
        let builder = Builder::new(|| Ok::<_, Infallible>(7_u8).map_err(|never| match never {}));
        let result: Result<u8, Failure> = builder.build();
        assert_eq!(result, Ok(7));
    }
}
