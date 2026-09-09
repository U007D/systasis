//! Exact type-level symbol keys, encoded as balanced padded UTF-8 bit trees.
//!
//! `Pad` is distinct from either bit, so a byte prefix cannot equal an extended
//! key, including an extension containing only NUL bytes. Namespaces and groups
//! are paired as separate components rather than concatenated. This represents
//! normalized source spelling, not semantic identity across trait re-exports.

use core::marker::PhantomData;

/// A true type-level answer.
pub struct Yes;
/// A false type-level answer.
pub struct No;
/// Conjunction of type-level answers.
pub trait And<R> {
    /// The resulting answer.
    type Out;
}
impl<R> And<R> for No {
    type Out = No;
}
impl<R> And<R> for Yes {
    type Out = R;
}
/// Disjunction of type-level answers.
pub trait Or<R> {
    /// The resulting answer.
    type Out;
}
impl<R> Or<R> for Yes {
    type Out = Yes;
}
impl<R> Or<R> for No {
    type Out = R;
}
/// A zero bit in the exact byte representation.
pub struct Zero;
/// A one bit in the exact byte representation.
pub struct One;
/// Padding, distinct from both bits and every branch.
pub struct Pad;
/// An ordered branch in a balanced key tree.
pub struct Pair<L, R>(PhantomData<fn() -> (L, R)>);
/// Structural equality of two exact keys.
pub trait Equal<R> {
    /// `Yes` for identical trees; otherwise `No`.
    type Out;
}
macro_rules! leaf {
    ($left:ty, $right:ty, $answer:ty) => {
        impl Equal<$right> for $left {
            type Out = $answer;
        }
    };
}
leaf!(Zero, Zero, Yes);
leaf!(Zero, One, No);
leaf!(Zero, Pad, No);
leaf!(One, Zero, No);
leaf!(One, One, Yes);
leaf!(One, Pad, No);
leaf!(Pad, Zero, No);
leaf!(Pad, One, No);
leaf!(Pad, Pad, Yes);
macro_rules! leaf_pair {
    ($leaf:ty) => {
        impl<L, R> Equal<Pair<L, R>> for $leaf {
            type Out = No;
        }
        impl<L, R> Equal<$leaf> for Pair<L, R> {
            type Out = No;
        }
    };
}
leaf_pair!(Zero);
leaf_pair!(One);
leaf_pair!(Pad);
impl<A, B, C, D> Equal<Pair<C, D>> for Pair<A, B>
where
    A: Equal<C>,
    B: Equal<D>,
    A::Out: And<B::Out>,
{
    type Out = <A::Out as And<B::Out>>::Out;
}

#[cfg(test)]
mod tests {
    use super::*;
    fn equal<L: Equal<R, Out = Yes>, R>() {}
    fn different<L: Equal<R, Out = No>, R>() {}
    type Nul =
        Pair<Pair<Pair<Zero, Zero>, Pair<Zero, Zero>>, Pair<Pair<Zero, Zero>, Pair<Zero, Zero>>>;
    type A = Pair<Pair<Pair<One, Zero>, Pair<Zero, Zero>>, Pair<Pair<Zero, Zero>, Pair<One, Zero>>>;

    #[test]
    fn structural_equality_preserves_bits_padding_and_shape() {
        equal::<Zero, Zero>();
        equal::<One, One>();
        equal::<Pad, Pad>();
        equal::<A, A>();
        different::<Zero, One>();
        different::<Zero, Pad>();
        different::<One, Pad>();
        different::<Pad, Nul>();
        different::<Nul, Pair<Nul, Nul>>();
        different::<A, Pair<A, Nul>>();
        different::<Pair<A, Pad>, Pair<A, Nul>>();
        different::<Pair<A, Nul>, Pair<Nul, A>>();
    }
}
