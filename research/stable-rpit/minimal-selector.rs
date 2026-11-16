#![forbid(unsafe_code, unstable_features)]

use core::error::Error;

trait IFunctionOutput {
    type Output;
}
impl<F, R> IFunctionOutput for F
where
    F: FnOnce() -> R,
{
    type Output = R;
}
type Never = <fn() -> ! as IFunctionOutput>::Output;

struct Select<const NEVER: bool>;
trait IOutput<E> {
    type Error;
}
impl<E> IOutput<E> for Select<false> {
    type Error = E;
}
impl IOutput<core::convert::Infallible> for Select<true> {
    type Error = Never;
}
type Chosen<E, const NEVER: bool> = <Select<NEVER> as IOutput<E>>::Error;

fn infallible() -> Result<u8, Chosen<impl Error, true>> {
    Ok::<u8, Never>(42)
}

fn fallible() -> Result<u8, Chosen<impl Error, false>> {
    "bad".parse::<u8>()
}

fn main() {
    let Ok(number) = infallible();
    assert_eq!(number, 42);
    assert!(fallible().is_err());
}
