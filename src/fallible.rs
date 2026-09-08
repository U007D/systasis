//! Explicit conversions for annotated fallible constructors.

/// The output and error of a fallible constructor.
///
/// Conversions are explicit: converting to an option discards error details.
/// Generated resolvers retain the constructor's annotated return type.
pub trait Fallible {
    /// Successfully constructed value.
    type Output;
    /// Failure information (`()` for an option).
    type Error;

    /// Preserve the success value or return the failure information.
    fn into_result(self) -> Result<Self::Output, Self::Error>;

    /// Preserve the success value, discarding failure information.
    fn into_option(self) -> Option<Self::Output>;
}

impl<T> Fallible for Option<T> {
    type Output = T;
    type Error = ();

    fn into_result(self) -> Result<T, ()> {
        self.ok_or(())
    }

    fn into_option(self) -> Option<T> {
        self
    }
}

impl<T, E> Fallible for Result<T, E> {
    type Output = T;
    type Error = E;

    fn into_result(self) -> Result<T, E> {
        self
    }

    fn into_option(self) -> Option<T> {
        self.ok()
    }
}
