//! Owned, explicitly typed constructor capture state.

/// Capture storage has exactly the auto traits of its stored tuple.
pub struct FactorySlot<C>(C);

impl<C> FactorySlot<C> {
    /// Move capture state into the factory without invoking it.
    pub const fn new(captures: C) -> Self {
        Self(captures)
    }

    /// Borrow capture state for a repeatable constructor invocation.
    pub const fn captures(&self) -> &C {
        &self.0
    }
}

/// State the existing capture restriction of a nongeneric native opaque alias.
///
/// Generated code does not call this for aliases with enclosing generic
/// parameters: those can represent externally borrowed capture types.
/// Keep the remedy beside the bound; rustc includes this source line in E0597's
/// explanatory note. This is a rendered diagnostic, not a
/// custom structured compiler message.
#[inline]
pub fn check_native_constructor_captures<F>(_: &F)
where
    F: 'static, // systasis capture limit: use an ordinary function for the constructor body; pass borrowed inputs as arguments.
{
}
