//! Deferred initialization into storage owned by the enclosing generated scope.

use core::cell::OnceCell;

/// A consuming initializer for the generated container's hidden owner.
///
/// Generated code creates one builder for a fresh, pinned owner and never
/// exposes that owner for replacement. This type borrows the owner; it does
/// not pin storage itself or permit the constructed value to borrow its own
/// fields. Pinning and the owner's enclosing scope are the generator's job.
pub struct Builder<'anchor, T, F> {
    owner: &'anchor OnceCell<T>,
    initialize: F,
}

impl<'anchor, T, F> Builder<'anchor, T, F> {
    /// Retain initialization without executing it.
    pub const fn new<E>(owner: &'anchor OnceCell<T>, initialize: F) -> Self
    where
        F: FnOnce() -> Result<T, E>,
    {
        Self { owner, initialize }
    }

    /// Initialize once and borrow the successful value from its enclosing owner.
    ///
    /// Failure leaves the owner empty. Initializer-local state and remaining
    /// captured inputs are dropped normally; values moved into the returned
    /// error remain owned by that error.
    pub fn build<E>(self) -> Result<&'anchor T, E>
    where
        F: FnOnce() -> Result<T, E>,
    {
        let value = (self.initialize)()?;
        self.owner.set(value).unwrap_or_else(|_| {
            unreachable!(
                "generated code gives a fresh owner exactly one builder, and build consumes that sole writer"
            )
        });
        Ok(self.owner.get().unwrap_or_else(|| {
            unreachable!(
                "successful OnceCell::set initialized the owner, and shared access cannot remove its value"
            )
        }))
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
        let owner = OnceCell::new();
        let calls = core::cell::Cell::new(0);
        let builder = Builder::new(&owner, || {
            calls.set(calls.get() + 1);
            Ok::<_, Infallible>(17)
        });

        assert_eq!(calls.get(), 0);
        assert!(owner.get().is_none());
        let value = builder.build().unwrap();
        assert_eq!(*value, 17);
        assert_eq!(calls.get(), 1);
        assert!(core::ptr::eq(value, owner.get().unwrap()));
    }

    #[test]
    fn abandoning_builder_drops_owned_captures_without_initializing() {
        let drops = core::cell::Cell::new(0);
        let owner = OnceCell::new();
        let capture = Tracked(&drops);
        let builder = Builder::new(&owner, move || Ok::<_, Infallible>(capture));

        assert_eq!(drops.get(), 0);
        drop(builder);
        assert_eq!(drops.get(), 1);
        assert!(owner.get().is_none());
    }

    #[test]
    #[allow(
        clippy::drop_non_drop,
        reason = "explicit abandonment must end the initializer's exclusive input borrow"
    )]
    fn abandoning_builder_releases_mutably_borrowed_inputs() {
        let owner = OnceCell::new();
        let mut input = 3;
        let builder = Builder::new(&owner, || {
            input += 1;
            Ok::<_, Infallible>(input)
        });

        drop(builder);
        input += 2;
        assert_eq!(input, 5, "the abandoned initializer did not run");
        assert!(owner.get().is_none());
    }

    #[test]
    fn building_releases_temporary_input_borrows_while_result_remains_live() {
        let owner = OnceCell::new();
        let mut input = 3;
        let builder = Builder::new(&owner, || {
            input += 1;
            Ok::<_, Infallible>(input)
        });

        let value = builder.build().unwrap();
        input += 2;
        assert_eq!((*value, input), (4, 6));
    }

    #[test]
    fn failure_preserves_its_type_and_cleans_up_partial_state_and_captures() {
        let owner = OnceCell::<u8>::new();
        let capture_drops = core::cell::Cell::new(0);
        let partial_drops = core::cell::Cell::new(0);
        let capture = Tracked(&capture_drops);
        let partial_counter = &partial_drops;
        let builder = Builder::new(&owner, move || {
            let _partial = Tracked(partial_counter);
            let _ = &capture;
            Err(Failure(7))
        });

        let result: Result<&u8, Failure> = builder.build();
        assert_eq!(result, Err(Failure(7)));
        assert_eq!(capture_drops.get(), 1);
        assert_eq!(partial_drops.get(), 1);
        assert!(owner.get().is_none());
    }

    #[test]
    fn failure_releases_temporary_mutable_input_borrows() {
        let owner = OnceCell::<u8>::new();
        let mut input = 3;
        let builder = Builder::new(&owner, || {
            input += 1;
            Err(Failure(7))
        });

        let result = builder.build();
        input += 2;
        assert_eq!(result, Err(Failure(7)));
        assert_eq!(input, 6, "initializer side effects are not rolled back");
        assert!(owner.get().is_none());
    }

    #[test]
    fn an_error_keeps_ownership_of_values_moved_into_it() {
        let owner = OnceCell::<()>::new();
        let drops = core::cell::Cell::new(0);
        let capture = Tracked(&drops);
        let builder = Builder::new(&owner, move || Err(capture));

        let Err(error) = builder.build() else {
            panic!("the initializer always returns its capture as an error");
        };
        assert_eq!(drops.get(), 0);
        assert!(owner.get().is_none());
        drop(error);
        assert_eq!(drops.get(), 1);
    }

    #[test]
    fn pinned_owner_accepts_a_non_unpin_value_without_unsafe_code() {
        struct Value {
            number: u8,
            _pin: PhantomPinned,
        }

        let owner = pin!(OnceCell::new());
        let storage = owner.as_ref().get_ref();
        let builder = Builder::new(storage, || {
            Ok::<_, Infallible>(Value {
                number: 7,
                _pin: PhantomPinned,
            })
        });
        let value = builder.build().unwrap();

        assert_eq!(value.number, 7);
        assert!(core::ptr::eq(value, storage.get().unwrap()));
    }

    fn error_type<T, E>(_: &Result<T, E>) -> &'static str {
        core::any::type_name::<E>()
    }

    #[test]
    fn empty_match_preserves_unconstrained_never_error_inference() {
        let owner = OnceCell::new();
        let builder = Builder::new(&owner, || {
            Ok::<_, Infallible>(7_u8).map_err(|never| match never {})
        });
        let result = builder.build();
        assert_eq!(error_type(&result), "!");
        let Ok(value) = result;
        assert_eq!(*value, 7);
    }

    #[test]
    fn placeholder_build_error_preserves_never_fallback() {
        let owner = OnceCell::new();
        let builder = Builder::new(&owner, || {
            Ok::<_, Infallible>(7_u8).map_err(|never| match never {})
        });
        let result = builder.build::<_>();
        assert_eq!(error_type(&result), "!");
    }

    #[test]
    fn explicit_build_error_type_supplies_initializer_context() {
        let owner = OnceCell::new();
        let builder = Builder::new(&owner, || {
            Ok::<_, Infallible>(7_u8).map_err(|never| match never {})
        });
        let result: Result<&u8, Failure> = builder.build::<Failure>();
        assert_eq!(result, Ok(&7));
    }

    #[test]
    fn surrounding_result_type_supplies_initializer_error_context() {
        let owner = OnceCell::new();
        let builder = Builder::new(&owner, || {
            Ok::<_, Infallible>(7_u8).map_err(|never| match never {})
        });
        let result: Result<&u8, Failure> = builder.build();
        assert_eq!(result, Ok(&7));
    }
}
