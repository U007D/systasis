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

// Called before assigning native aliases without enclosing generics. Checking
// the concrete closure makes local E0597 errors point to this bound and remedy.
// Keep the comment on the bound's line: rustc renders that source excerpt.
/// Systasis cannot yet represent some borrowed constructor inputs' lifetimes.
/// Workaround: register a non-borrowing implementation with owned fields and captures.
/// Not all affected compiler errors include this guidance.
#[inline]
pub fn check_native_constructor_captures<F>(constructor: F) -> F
where
    F: 'static, // systasis cannot store this borrowed capture here; register a non-borrowing implementation.
{
    constructor
}
