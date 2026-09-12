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
/// Some constructors that capture references are not supported yet: the generated
/// closure storage cannot represent their borrowed inputs' lifetimes. This can
/// cause E0597 or E0521 even when the input lives long enough for the intended use.
/// It is a systasis implementation limitation, not proof of an invalid user borrow.
///
/// A simple, coarse workaround is to register a non-borrowing implementation:
/// use owned fields and owned constructor captures, e.g. `String` instead of `&str`.
/// Adding `move` to a closure that captures a reference still moves a reference;
/// it does not make the referenced data owned. See docs/CAPTURE_LIMITS.md.
///
/// Generated code does not call this for aliases with enclosing generic
/// parameters: those can represent externally borrowed capture types.
/// Keep the remedy beside the bound; rustc includes this source line in E0597's
/// explanatory note. E0521 does not show it. Diagnostic coverage is incomplete:
/// this is a rendered source excerpt, not a custom structured compiler message.
#[inline]
pub fn check_native_constructor_captures<F>(_: &F)
where
    F: 'static, // systasis cannot store this borrowed capture here; register a non-borrowing implementation.
{
}
