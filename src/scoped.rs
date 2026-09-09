//! Safe type metadata and dispatch used by generated child scopes.
//!
//! These traits do not discover registrations at runtime or return unrestricted
//! child references. Their implementations carry Rust types through aliases.

use crate::{Ref, RefMut};

pub mod key;
pub mod mask;

/// Equality witness used to retain ordinary Rust visibility in metadata impls.
pub trait Identity {
    /// The original type, without wrapping a runtime value.
    type Type: ?Sized;
    /// Return the original value through the associated-type equality.
    fn into_identity(self) -> Self::Type
    where
        Self: Sized,
        Self::Type: Sized;
}
impl<T: ?Sized> Identity for T {
    type Type = T;
    fn into_identity(self) -> T
    where
        Self: Sized,
    {
        self
    }
}

/// Lifetime-specific metadata underlying the public generic associated types.
pub trait MetadataAt<'a, Path, Key, Kind> {
    /// The selected value, result, or trait-object target.
    type Value: ?Sized;
}
/// Identifies concrete registration metadata rather than an operation result.
pub struct RegisteredValue;
/// Identifies opted-in trait-object metadata.
pub struct DynamicTarget;
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

/// Ownership exclusions associated with borrowing one registration by path.
pub trait Borrowed<Path, Key> {
    /// Local or nested mask preserving the registration's namespace.
    type Mask;
}

/// A registration's concrete value type, including borrowed constructor output.
pub trait Registered<Path, Key> {
    /// The value corresponding to a container borrow of this lifetime.
    type Value<'a>
    where
        Self: 'a;
}

/// Declaration-site Copy policy for a registration resolved through a path.
///
/// The generated implementation selects the original registration's policy;
/// `LOCAL` selects the receiving container's non-Copy storage backend.
pub trait RegistrationPolicy<Path, Key, const LOCAL: bool> {
    /// A type implementing the existing storage selection contract.
    type Policy;
}

/// Recover a stored slot's selected Copy policy without redetecting its value.
///
/// Fresh constructors do not implement this trait: their output policy must
/// be supplied by the generator from the declaration-site bounds.
pub trait SlotPolicy<const LOCAL: bool> {
    /// The policy adapted to the receiving container's local setting.
    type Policy;
}

/// Change only the local-storage setting of an existing declaration policy.
#[doc(hidden)]
pub trait RebindPolicy<const LOCAL: bool> {
    /// The same Copy decision with the requested local-storage setting.
    type Policy;
}
impl<const COPY: bool, const OLD: bool, const LOCAL: bool> RebindPolicy<LOCAL>
    for crate::__private::Policy<COPY, OLD>
{
    type Policy = crate::__private::Policy<COPY, LOCAL>;
}
impl<T, const LOCAL: bool> SlotPolicy<LOCAL> for CopySlot<T> {
    type Policy = crate::__private::Policy<true, LOCAL>;
}
impl<T, const LOCAL: bool> SlotPolicy<LOCAL> for TakeSlot<T> {
    type Policy = crate::__private::Policy<false, LOCAL>;
}
impl<T, const LOCAL: bool> SlotPolicy<LOCAL> for LocalTakeSlot<T> {
    type Policy = crate::__private::Policy<false, LOCAL>;
}
impl<T, const LOCAL: bool> SlotPolicy<LOCAL> for ReadSlot<T> {
    type Policy = crate::__private::Policy<false, LOCAL>;
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

/// Add restrictions to an existing descriptor without extending its backing lifetime.
pub trait ReScope<Restrictions> {
    /// A descriptor retaining both the old and new exclusions.
    type Scope;
    /// Construct the more-restricted descriptor from its existing backing reference.
    fn rescope(&self) -> Self::Scope;
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

/// Explicit forwarding of an already-authorized unchecked scope operation.
#[cfg(feature = "resolve_unchecked")]
pub trait UnsafeResolve<'backing, Path, Key, Operation> {
    /// The operation's owned value or retained guard.
    type Output;
    /// Perform synchronized nonblocking access without returning access errors.
    ///
    /// # Safety
    /// The value must be present and the selected lock acquisition must succeed.
    /// This operation must not bypass the scope's compile-time exclusions.
    unsafe fn resolve(&self) -> Self::Output;
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
    /// Unchecked ownership transfer under the slot's existing synchronization.
    #[cfg(feature = "resolve_unchecked")]
    pub struct UncheckedOwned;
    /// Unchecked shared acquisition retaining its guard.
    #[cfg(feature = "resolve_unchecked")]
    pub struct UncheckedShared;
    /// Unchecked exclusive acquisition retaining its guard.
    #[cfg(feature = "resolve_unchecked")]
    pub struct UncheckedExclusive;
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
