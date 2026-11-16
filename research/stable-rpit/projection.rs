#![forbid(unsafe_code, unstable_features)]

use std::error::Error;
use std::marker::PhantomData;

trait FunctionOutput {
    type Output;
}
impl<F, R> FunctionOutput for F
where
    F: FnOnce() -> R,
{
    type Output = R;
}
type Never = <fn() -> ! as FunctionOutput>::Output;

trait Kind {
    const NEVER: bool;
}
struct Impossible;
impl Kind for Impossible {
    const NEVER: bool = true;
}
struct Possible<E>(PhantomData<E>);
impl<E> Kind for Possible<E> {
    const NEVER: bool = false;
}

const fn is_never<F, K>(_: &F) -> bool
where
    F: FnOnce() -> K,
    K: Kind,
{
    K::NEVER
}

struct Select<const NEVER: bool>;
trait Choose<E> {
    type Error;
    fn convert(e: E) -> Self::Error;
}
impl<E> Choose<E> for Select<false> {
    type Error = E;
    fn convert(e: E) -> E {
        e
    }
}
impl<E> Choose<E> for Select<true> {
    type Error = Never;
    fn convert(_: E) -> Never {
        unreachable!("only selected for an uninhabited source")
    }
}

fn impossible_kind() -> impl Kind {
    Impossible
}
fn possible_kind() -> impl Kind {
    Possible::<std::num::ParseIntError>(PhantomData)
}

type Selected<E, const NEVER: bool> = <Select<NEVER> as Choose<E>>::Error;
struct Container<E> {
    value: u8,
    error_type: PhantomData<fn() -> E>,
}
type BuildResult<E, const NEVER: bool> = Result<Container<E>, Selected<E, NEVER>>;

fn infallible() -> BuildResult<impl Error, { is_never(&impossible_kind) }> {
    let result: Result<u8, Never> = Ok(42);
    result
        .map(|value| Container::<Never> {
            value,
            error_type: PhantomData,
        })
        .map_err(<Select<{ is_never(&impossible_kind) }> as Choose<Never>>::convert)
}

fn fallible() -> BuildResult<impl Error, { is_never(&possible_kind) }> {
    "bad"
        .parse::<u8>()
        .map(|value| Container::<std::num::ParseIntError> {
            value,
            error_type: PhantomData,
        })
        .map_err(<Select<{ is_never(&possible_kind) }> as Choose<_>>::convert)
}

fn main() {
    let Ok(value) = infallible();
    assert_eq!(value.value, 42);
    assert!(fallible().is_err());
}
