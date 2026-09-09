//! Safe type metadata and dispatch used by generated child scopes.
//!
//! These traits do not discover registrations at runtime or return unrestricted
//! child references. Their implementations carry Rust types through aliases.

use crate::{Ref, RefMut};

pub mod key;
pub mod mask;
use core::cell::{Ref as LocalRef, RefMut as LocalRefMut};
use core::marker::PhantomData;

use crate::{
    app_container::Error,
    fresh::FreshSlot,
    storage::{CopySlot, LocalTakeSlot, ReadSlot, TakeSlot},
};

/// The implicit namespace, also selected by explicitly spelling `default`.
pub struct DefaultNamespace;

/// A local registration lookup within a particular namespace.
pub struct Here<Namespace = DefaultNamespace>(PhantomData<fn() -> Namespace>);

/// A nested child traversal followed by the remaining lookup path.
pub struct There<ChildKey, Rest = Here>(PhantomData<fn() -> (ChildKey, Rest)>);

/// Metadata identifying a child type without exposing a child reference.
pub trait Child<Key> {
    /// The independently owned child's container type.
    type Container: ?Sized;
}

/// A registration's concrete value type, including borrowed constructor output.
pub trait Registered<Path, Key> {
    /// The value corresponding to a container borrow of this lifetime.
    type Value<'a>
    where
        Self: 'a;
}

/// The explicit trait-object target of an opted-in registration.
pub trait DynRegistered<Path, Key> {
    /// A single or generated combined trait-object type.
    type Target<'a>: ?Sized
    where
        Self: 'a;
}

/// Exact result metadata for a particular operation, not necessarily a value.
pub trait Output<Path, Key, Operation> {
    /// Preserves the selected operation's guard, Result, Option, or owned type.
    type Value<'a>
    where
        Self: 'a;
}

/// Constructs a descriptor which the parent can store inline.
///
/// Generated public child accessors return references to the stored descriptor;
/// they do not expose this construction step. The descriptor must keep backing
/// references private and must not provide Deref/AsRef to unrestricted backing.
/// A restricted descriptor must not implement a conversion that weakens its
/// existing exclusions; nested scope construction must combine restrictions.
pub trait AsScope<Restrictions> {
    /// A descriptor borrowing this independently owned container.
    type Scope<'a>
    where
        Self: 'a;
    /// Construct a descriptor without moving its backing container.
    fn scope(&self) -> Self::Scope<'_>;
}

/// Resolution through a restricted scope, borrowing the backing container.
///
/// The explicit lifetime permits a returned guard to outlive the descriptor
/// itself while its independently owned backing container remains borrowed.
pub trait Resolve<'backing, Path, Key, Operation> {
    /// The selected operation's exact return type.
    type Output;
    /// Perform the operation using its existing nonblocking access rules.
    fn resolve(&self) -> Self::Output;
}

/// Type-level operation names; none select a runtime dispatch branch.
pub mod op {
    /// Repeatable ownership from Copy storage or a fresh constructor.
    pub struct Owned;
    /// Fallible ownership from consumable storage or a fallible constructor.
    pub struct TryOwned;
    /// Infallible shared reference to plain storage.
    pub struct Shared;
    /// Checked shared guard acquisition.
    pub struct TryShared;
    /// Checked exclusive guard acquisition.
    pub struct TryExclusive;
    /// Explicit infallible cloning from plain storage.
    pub struct CloneValue;
    /// Explicit cloning under checked shared access.
    pub struct TryCloneValue;
    /// Infallible shared reference to an opted-in trait-object target.
    pub struct DynShared;
    /// Checked shared guard mapped to an opted-in trait-object target.
    pub struct TryDynShared;
}

/// Value metadata for ordinary slots; factories provide their own output GATs.
pub trait SlotValue {
    /// The slot's stored or default-constructed type.
    type Value;
}

/// A supported checked operation on one storage slot.
pub trait SlotAccess<Operation> {
    /// Exact operation output tied to the slot borrow where necessary.
    type Output<'a>
    where
        Self: 'a;
    /// Use the slot's existing implementation, retaining any returned guard.
    fn access(&self) -> Self::Output<'_>;
}

macro_rules! slot_values {
    ($($slot:ident),* $(,)?) => {$(
        impl<T> SlotValue for $slot<T> { type Value = T; }
    )*};
}
slot_values!(CopySlot, ReadSlot, TakeSlot, LocalTakeSlot, FreshSlot);

impl<T: Copy> SlotAccess<op::Owned> for CopySlot<T> {
    type Output<'a>
        = T
    where
        Self: 'a;
    fn access(&self) -> T {
        self.resolve()
    }
}
impl<T: Copy> SlotAccess<op::Shared> for CopySlot<T> {
    type Output<'a>
        = &'a T
    where
        Self: 'a;
    fn access(&self) -> &T {
        self.resolve_ref()
    }
}
impl<T: Copy> SlotAccess<op::CloneValue> for CopySlot<T> {
    type Output<'a>
        = T
    where
        Self: 'a;
    fn access(&self) -> T {
        self.resolve_clone()
    }
}
impl<T> SlotAccess<op::Shared> for ReadSlot<T> {
    type Output<'a>
        = &'a T
    where
        Self: 'a;
    fn access(&self) -> &T {
        self.resolve_ref()
    }
}
impl<T: Clone> SlotAccess<op::CloneValue> for ReadSlot<T> {
    type Output<'a>
        = T
    where
        Self: 'a;
    fn access(&self) -> T {
        self.resolve_clone()
    }
}
impl<T: Default> SlotAccess<op::Owned> for FreshSlot<T> {
    type Output<'a>
        = T
    where
        Self: 'a;
    fn access(&self) -> T {
        self.resolve()
    }
}

macro_rules! checked_slot {
    ($slot:ident, $shared:ident, $exclusive:ident) => {
        impl<T> SlotAccess<op::TryOwned> for $slot<T> {
            type Output<'a>
                = Result<T, Error>
            where
                Self: 'a;
            fn access(&self) -> Self::Output<'_> {
                self.try_resolve()
            }
        }
        impl<T> SlotAccess<op::TryShared> for $slot<T> {
            type Output<'a>
                = Result<$shared<'a, T>, Error>
            where
                Self: 'a;
            fn access(&self) -> Self::Output<'_> {
                self.try_resolve_ref()
            }
        }
        impl<T> SlotAccess<op::TryExclusive> for $slot<T> {
            type Output<'a>
                = Result<$exclusive<'a, T>, Error>
            where
                Self: 'a;
            fn access(&self) -> Self::Output<'_> {
                self.try_resolve_ref_mut()
            }
        }
        impl<T: Clone> SlotAccess<op::TryCloneValue> for $slot<T> {
            type Output<'a>
                = Result<T, Error>
            where
                Self: 'a;
            fn access(&self) -> Self::Output<'_> {
                self.try_resolve_clone()
            }
        }
    };
}
checked_slot!(TakeSlot, Ref, RefMut);
checked_slot!(LocalTakeSlot, LocalRef, LocalRefMut);
