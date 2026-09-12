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

// Called only for native aliases without enclosing generics. Keep the remedy
// beside the bound: rustc displays this source excerpt for E0597, but not E0521.
/// Systasis cannot yet represent some borrowed constructor inputs' lifetimes.
/// Workaround: register a non-borrowing implementation with owned fields and captures.
/// Not all affected compiler errors include this guidance.
#[inline]
pub fn check_native_constructor_captures<F>(_: &F)
where
    F: 'static, // systasis cannot store this borrowed capture here; register a non-borrowing implementation.
{
}
