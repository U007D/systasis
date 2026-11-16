//! A hand-expanded stable representation; this is not a Systasis integration.
#![no_std]
#![forbid(unsafe_code, unstable_features)]

#[cfg(feature = "local_probes")]
extern crate alloc;

#[cfg(feature = "local_probes")]
use alloc::rc::Rc;
use core::cell::Cell;
use core::error::Error;
use core::fmt;
use core::marker::PhantomData;
use core::sync::atomic::{AtomicUsize, Ordering};

static INITIALIZATIONS: AtomicUsize = AtomicUsize::new(0);
static CALLS: AtomicUsize = AtomicUsize::new(0);
static DROPS: AtomicUsize = AtomicUsize::new(0);

struct CaptureProbe;
impl CaptureProbe {
    fn new() -> Self {
        INITIALIZATIONS.fetch_add(1, Ordering::SeqCst);
        Self
    }
    fn called(&self) {
        CALLS.fetch_add(1, Ordering::SeqCst);
    }
}
impl Drop for CaptureProbe {
    fn drop(&mut self) {
        DROPS.fetch_add(1, Ordering::SeqCst);
    }
}
pub fn reset_counts() {
    INITIALIZATIONS.store(0, Ordering::SeqCst);
    CALLS.store(0, Ordering::SeqCst);
    DROPS.store(0, Ordering::SeqCst);
}
pub fn counts() -> (usize, usize, usize) {
    (
        INITIALIZATIONS.load(Ordering::SeqCst),
        CALLS.load(Ordering::SeqCst),
        DROPS.load(Ordering::SeqCst),
    )
}

pub trait IFunctionOutput {
    type Output;
}
impl<F, R> IFunctionOutput for F
where
    F: FnOnce() -> R,
{
    type Output = R;
}
pub type Never = <fn() -> ! as IFunctionOutput>::Output;

pub trait IErrorKind {
    const NEVER: bool;
}
pub struct Impossible;
impl IErrorKind for Impossible {
    const NEVER: bool = true;
}
pub struct Possible<E>(PhantomData<E>);
impl<E> IErrorKind for Possible<E> {
    const NEVER: bool = false;
}
pub struct Infer<E>(PhantomData<E>);
impl Infer<Never> {
    pub fn kind(self) -> Impossible {
        Impossible
    }
}
pub trait IActualError {
    type Error;
    fn kind(self) -> Possible<Self::Error>;
}
impl<E> IActualError for Infer<E> {
    type Error = E;
    fn kind(self) -> Possible<E> {
        Possible(PhantomData)
    }
}
pub struct Kinds<A, B>(A, B);
impl<A, B> IErrorKind for Kinds<A, B>
where
    A: IErrorKind,
    B: IErrorKind,
{
    const NEVER: bool = A::NEVER && B::NEVER;
}

pub const fn is_never<F, K>(_: &F) -> bool
where
    F: FnOnce() -> K,
    K: IErrorKind,
{
    K::NEVER
}

pub struct Select<const NEVER: bool>;
pub trait IChoose<E> {
    type Error;
    fn convert(error: E) -> Self::Error;
}
impl<E> IChoose<E> for Select<false> {
    type Error = E;
    fn convert(error: E) -> E {
        error
    }
}
impl<E> IChoose<E> for Select<true> {
    type Error = Never;
    fn convert(_: E) -> Never {
        unreachable!(
            "the generated kind helper checks the same initializers; every source error is never"
        )
    }
}
pub type Selected<E, const NEVER: bool> = <Select<NEVER> as IChoose<E>>::Error;

#[derive(Debug)]
pub enum Sources<A, B> {
    First(A),
    Second(B),
}
impl<A, B> fmt::Display for Sources<A, B>
where
    A: Error,
    B: Error,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::First(e) => fmt::Display::fmt(e, f),
            Self::Second(e) => fmt::Display::fmt(e, f),
        }
    }
}
impl<A, B> Error for Sources<A, B>
where
    A: Error + 'static,
    B: Error + 'static,
{
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(match self {
            Self::First(e) => e,
            Self::Second(e) => e,
        })
    }
}
pub fn first<T, A, B>(
    value: Result<T, A>,
    _: &mut PhantomData<Sources<A, B>>,
) -> Result<T, Sources<A, B>> {
    value.map_err(Sources::First)
}
pub fn second<T, A, B>(
    value: Result<T, B>,
    _: &mut PhantomData<Sources<A, B>>,
) -> Result<T, Sources<A, B>> {
    value.map_err(Sources::Second)
}
pub fn first_kind<T, A, B>(_: &Result<T, Sources<A, B>>) -> Infer<A> {
    Infer(PhantomData)
}
pub fn second_kind<T, A, B>(_: &Result<T, Sources<A, B>>) -> Infer<B> {
    Infer(PhantomData)
}
pub fn tie<T, E>(result: Result<T, E>, _: &PhantomData<E>) -> Result<T, E> {
    result
}

pub fn infallible_source() -> Result<u8, Never> {
    Ok(7)
}
pub fn opaque_error() -> Result<u8, impl Error> {
    "invalid".parse::<u8>()
}

pub trait IConfig {
    fn value(&self) -> u8;
}
impl IConfig for u8 {
    fn value(&self) -> u8 {
        *self
    }
}
pub fn opaque_config() -> impl IConfig {
    42u8
}

#[derive(Debug)]
#[cfg(feature = "local_probes")]
pub struct LocalError(Rc<()>);
#[cfg(feature = "local_probes")]
impl fmt::Display for LocalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("local failure")
    }
}
#[cfg(feature = "local_probes")]
impl Error for LocalError {}
#[cfg(feature = "local_probes")]
impl LocalError {
    pub fn owners(&self) -> usize {
        Rc::strong_count(&self.0)
    }
}
#[cfg(feature = "local_probes")]
pub fn local_source() -> Result<u8, LocalError> {
    Err(LocalError(Rc::new(())))
}
#[cfg(feature = "local_probes")]
pub fn local_source_ok() -> Result<u8, LocalError> {
    Ok(7)
}

macro_rules! declaration {
    ($module:ident, $first:expr, $second:expr, $factory:expr) => {
        pub mod $module {
            #[allow(unused_imports)]
            use crate::IActualError as _;
            use core::error::Error;
            use core::marker::PhantomData;

            pub struct SystasisContainer<F = (), E = ()> {
                factory: F,
                // The error type has no runtime value in a successful container.
                error_type: PhantomData<fn() -> E>,
            }
            pub type SystasisContainerError<E> = crate::Selected<E, { crate::is_never(&kind) }>;
            pub type BuildResult<F, E> = Result<SystasisContainer<F, E>, SystasisContainerError<E>>;

            macro_rules! initialize {
                () => {{
                    let mut marker = PhantomData;
                    let result = (|| {
                        let capture = crate::CaptureProbe::new();
                        let _ = crate::first($first, &mut marker)?;
                        let _ = crate::second($second, &mut marker)?;
                        let factory = $factory;
                        Ok(move || {
                            capture.called();
                            factory()
                        })
                    })();
                    crate::tie(result, &marker)
                }};
            }

            fn kind() -> impl crate::IErrorKind {
                let result = initialize!();
                crate::Kinds(
                    crate::first_kind(&result).kind(),
                    crate::second_kind(&result).kind(),
                )
            }

            fn finish<F, E>(result: Result<F, E>) -> BuildResult<F, E> {
                result
                    .map(|factory| SystasisContainer {
                        factory,
                        error_type: PhantomData,
                    })
                    .map_err(
                        <crate::Select<{ crate::is_never(&kind) }> as crate::IChoose<E>>::convert,
                    )
            }

            impl SystasisContainer {
                pub fn build() -> BuildResult<impl Fn() -> u8, impl Error + 'static> {
                    finish(initialize!())
                }
            }
            impl<F, E> SystasisContainer<F, E>
            where
                F: Fn() -> u8,
            {
                pub fn from_factory(factory: F) -> Self {
                    Self {
                        factory,
                        error_type: PhantomData,
                    }
                }
                pub fn resolve_i_number(&self) -> u8 {
                    (self.factory)()
                }
            }
        }
    };
}

declaration!(
    infallible,
    crate::infallible_source(),
    crate::infallible_source(),
    {
        use crate::IConfig as _;
        let config = crate::opaque_config();
        move || config.value()
    }
);
declaration!(
    parse_failure,
    "invalid".parse::<u8>(),
    core::str::from_utf8(&[65]),
    || 42u8
);
declaration!(
    utf8_failure,
    "42".parse::<u8>(),
    core::str::from_utf8(core::hint::black_box(&[255])),
    || 42u8
);
declaration!(
    mixed_success,
    "42".parse::<u8>(),
    core::str::from_utf8(&[65]),
    || 42u8
);
declaration!(
    opaque_failure,
    crate::opaque_error(),
    crate::infallible_source(),
    || 42u8
);
#[cfg(feature = "local_probes")]
declaration!(
    local_failure,
    crate::local_source(),
    crate::infallible_source(),
    || 42u8
);
#[cfg(feature = "local_probes")]
declaration!(
    local_error_success,
    crate::local_source_ok(),
    crate::infallible_source(),
    || 42u8
);
#[cfg(feature = "local_probes")]
declaration!(
    local_capture,
    crate::infallible_source(),
    crate::infallible_source(),
    {
        let config = alloc::rc::Rc::new(42u8);
        move || *config
    }
);

pub struct Tracked<'a> {
    pub drops: &'a Cell<u8>,
    pub calls: &'a Cell<u8>,
}
impl Tracked<'_> {
    pub fn value(&self) -> u8 {
        self.calls.set(self.calls.get().saturating_add(1));
        42
    }
}
impl Drop for Tracked<'_> {
    fn drop(&mut self) {
        self.drops.set(self.drops.get().saturating_add(1));
    }
}

pub fn tracked<'a>(owner: Tracked<'a>) -> impl Fn() -> u8 + 'a {
    move || owner.value()
}

pub fn borrowed(value: &u8) -> infallible::SystasisContainer<impl Fn() -> u8 + '_, Never> {
    infallible::SystasisContainer::from_factory(move || *value)
}
