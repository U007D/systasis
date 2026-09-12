//! Type-level restrictions retained when traversing generated child scopes.

use super::key::{Equal, No, Or, Yes};
use core::marker::PhantomData;

/// No restricted registrations.
pub struct Empty;
/// A restricted local key followed by further restrictions.
pub struct Mask<Key, Tail>(PhantomData<fn() -> (Key, Tail)>);
/// Restrictions from either component apply.
pub struct Union<A, B>(PhantomData<fn() -> (A, B)>);
/// Restrictions belonging to one child rather than the current scope.
pub struct Nested<Key, M>(PhantomData<fn() -> (Key, M)>);

/// Indirectly name the dependencies borrowed by a registration.
///
/// Keeping the container type here defers its associated mask projection until
/// a restriction operation is requested. Both operations forward unchanged.
#[doc(hidden)]
pub struct BorrowedBy<C, Path, Key> {
    container: PhantomData<fn() -> C>,
    registration: PhantomData<fn() -> (Path, Key)>,
}

impl<C, Path, Key, Query> Blocked<Query> for BorrowedBy<C, Path, Key>
where
    C: super::Borrowed<Path, Key>,
    C::Mask: Blocked<Query>,
{
    type Out = <C::Mask as Blocked<Query>>::Out;
}

impl<C, Path, Key, Query> ForChild<Query> for BorrowedBy<C, Path, Key>
where
    C: super::Borrowed<Path, Key>,
    C::Mask: ForChild<Query>,
{
    type Out = <C::Mask as ForChild<Query>>::Out;
}

/// Whether a local key is restricted.
pub trait Blocked<Key> {
    /// `Yes` when restricted; otherwise `No`.
    type Out;
}
impl<Key> Blocked<Key> for Empty {
    type Out = No;
}
impl<Key, H, T: Blocked<Key>> Blocked<Key> for Mask<H, T>
where
    Key: Equal<H>,
    <Key as Equal<H>>::Out: Or<T::Out>,
{
    type Out = <<Key as Equal<H>>::Out as Or<T::Out>>::Out;
}
impl<Key, A: Blocked<Key>, B: Blocked<Key>> Blocked<Key> for Union<A, B>
where
    A::Out: Or<B::Out>,
{
    type Out = <A::Out as Or<B::Out>>::Out;
}
impl<Key, H, M> Blocked<Key> for Nested<H, M> {
    type Out = No;
}
/// Select a child restriction set from an equality answer.
pub trait SelectMask<M> {
    /// The matching mask or `Empty`.
    type Out;
}
impl<M> SelectMask<M> for Yes {
    type Out = M;
}
impl<M> SelectMask<M> for No {
    type Out = Empty;
}
/// Retain restrictions that belong to the selected child.
pub trait ForChild<Key> {
    /// The child's restriction set.
    type Out;
}
impl<Key> ForChild<Key> for Empty {
    type Out = Empty;
}
impl<Key, H, T: ForChild<Key>> ForChild<Key> for Mask<H, T> {
    type Out = T::Out;
}
impl<Key, A: ForChild<Key>, B: ForChild<Key>> ForChild<Key> for Union<A, B> {
    type Out = Union<A::Out, B::Out>;
}
impl<Key, H, M> ForChild<Key> for Nested<H, M>
where
    Key: Equal<H>,
    <Key as Equal<H>>::Out: SelectMask<M>,
{
    type Out = <<Key as Equal<H>>::Out as SelectMask<M>>::Out;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scoped::key::{One, Pad, Zero};
    fn blocked<M: Blocked<K, Out = Yes>, K>() {}
    fn allowed<M: Blocked<K, Out = No>, K>() {}

    #[test]
    fn local_and_union_restrictions_are_additive() {
        type Both = Union<Mask<Zero, Empty>, Mask<One, Empty>>;
        allowed::<Empty, Zero>();
        blocked::<Both, Zero>();
        blocked::<Both, One>();
        allowed::<Both, Pad>();
    }

    #[test]
    fn child_traversal_drops_parent_local_restrictions_but_retains_nested_ones() {
        type Restrictions =
            Mask<Pad, Union<Nested<Zero, Mask<One, Empty>>, Nested<One, Mask<Zero, Empty>>>>;
        type Left = <Restrictions as ForChild<Zero>>::Out;
        type Right = <Restrictions as ForChild<One>>::Out;
        type Missing = <Restrictions as ForChild<Pad>>::Out;
        blocked::<Restrictions, Pad>();
        allowed::<Restrictions, Zero>();
        blocked::<Left, One>();
        allowed::<Left, Pad>();
        blocked::<Right, Zero>();
        allowed::<Missing, One>();
    }

    #[test]
    fn indirect_borrowed_masks_preserve_local_and_nested_restrictions() {
        use crate::scoped::{Borrowed, Here, Identity};
        struct Container;
        type Restrictions =
            Mask<Pad, Union<Nested<Zero, Mask<One, Empty>>, Nested<One, Mask<Zero, Empty>>>>;
        impl Borrowed<Here, Pad> for Container {
            type Mask = Restrictions;
        }
        type Indirect = BorrowedBy<Container, Here, Pad>;
        fn same<A: Identity<Type = B>, B>() {}
        same::<<Indirect as Blocked<Pad>>::Out, <Restrictions as Blocked<Pad>>::Out>();
        same::<<Indirect as ForChild<Zero>>::Out, <Restrictions as ForChild<Zero>>::Out>();
        same::<<Indirect as ForChild<One>>::Out, <Restrictions as ForChild<One>>::Out>();
        same::<<Indirect as ForChild<Pad>>::Out, <Restrictions as ForChild<Pad>>::Out>();
        blocked::<Indirect, Pad>();
        allowed::<Indirect, Zero>();
        blocked::<<Indirect as ForChild<Zero>>::Out, One>();
        blocked::<<Indirect as ForChild<One>>::Out, Zero>();
        allowed::<<Indirect as ForChild<Pad>>::Out, One>();
        blocked::<Union<Indirect, Mask<Zero, Empty>>, Zero>();
    }
}
