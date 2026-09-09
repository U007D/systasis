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
