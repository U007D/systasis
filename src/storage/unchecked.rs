//! Unchecked result handling preserves the checked acquisition mechanism.

use super::{LocalTakeSlot, TakeSlot};
use super::{Ref, RefMut};
use core::cell::{Ref as LocalRef, RefMut as LocalRefMut};

macro_rules! accessors {
    ($slot:ident, $read:ident, $write:ident) => {
        impl<T> $slot<T> {
            /// Takes the value without returning an access error.
            ///
            /// # Safety
            /// The value must be present and exclusive acquisition must succeed.
            /// No incompatible borrow or reservation may remain outstanding.
            pub unsafe fn resolve_unchecked(&self) -> T {
                self.try_resolve().unwrap_or_else(|_| {
                    unreachable!(
                        "caller guarantees a present value and successful exclusive acquisition"
                    )
                })
            }

            /// Borrows the value while retaining the shared access guard.
            ///
            /// # Safety
            /// The value must be present and shared acquisition must succeed.
            pub unsafe fn resolve_ref_unchecked(&self) -> $read<'_, T> {
                self.try_resolve_ref().unwrap_or_else(|_| {
                    unreachable!(
                        "caller guarantees a present value and successful shared acquisition"
                    )
                })
            }

            /// Mutably borrows the value while retaining the exclusive guard.
            ///
            /// # Safety
            /// The value must be present and exclusive acquisition must succeed.
            /// No incompatible borrow or reservation may remain outstanding.
            pub unsafe fn resolve_ref_mut_unchecked(&self) -> $write<'_, T> {
                self.try_resolve_ref_mut().unwrap_or_else(|_| {
                    unreachable!(
                        "caller guarantees a present value and successful exclusive acquisition"
                    )
                })
            }
        }
    };
}

accessors!(TakeSlot, Ref, RefMut);
accessors!(LocalTakeSlot, LocalRef, LocalRefMut);
