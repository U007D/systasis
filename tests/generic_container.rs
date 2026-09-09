//! Generic declarations retain their registration-site API after instantiation.
#![forbid(unsafe_code)]

use systasis::app_container::Error;

trait IValue {}
impl<T> IValue for T {}

mod unbounded {
    use super::*;

    #[systasis::container]
    pub fn run<T: PartialEq + core::fmt::Debug>(value: T, expected: T) {
        let Ok(container) = systasis::systasis_container! {
            register_value!(value: T as IValue);
        }
        .build();
        assert_eq!(container.try_resolve_i_value().unwrap(), expected);
        assert!(matches!(
            container.try_resolve_i_value(),
            Err(Error::ValueAlreadyConsumed)
        ));
    }
}

mod bounded {
    use super::*;

    #[systasis::container]
    pub fn run<T: Copy + PartialEq + core::fmt::Debug>(value: T) {
        let Ok(container) = systasis::systasis_container! {
            register_value!(value: T as IValue);
        }
        .build();
        assert_eq!(container.resolve_i_value(), value);
        assert_eq!(container.resolve_i_value(), value);
        assert_eq!(*container.resolve_i_value_ref(), value);
        super::named_parameter(container, value);
    }
}

fn named_parameter<T: Copy + PartialEq + core::fmt::Debug>(
    container: &bounded::AppContainer<T>,
    expected: T,
) {
    assert_eq!(container.resolve_i_value(), expected);
}

mod wrapper {
    use super::*;
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Wrapper<T>(pub T);

    #[systasis::container]
    pub fn run<T>(value: Wrapper<T>)
    where
        Wrapper<T>: Copy + PartialEq + core::fmt::Debug,
    {
        let Ok(container) = systasis::systasis_container! {
            register_value!(value: Wrapper<T> as IValue);
        }
        .build();
        assert_eq!(container.resolve_i_value(), value);
        assert_eq!(container.resolve_i_value(), value);
    }
}

mod associated {
    use super::*;
    #[systasis::container]
    pub fn run<T: Iterator>(value: T::Item)
    where
        T::Item: Copy + PartialEq + core::fmt::Debug,
    {
        let Ok(container) = systasis::systasis_container! {
            register_value!(value: T::Item as IValue);
        }
        .build();
        assert_eq!(container.resolve_i_value(), value);
        assert_eq!(container.resolve_i_value(), value);
    }
}

mod array {
    use super::*;
    #[systasis::container]
    pub fn run<T, const N: usize>(value: [T; N])
    where
        [T; N]: Copy + PartialEq + core::fmt::Debug,
    {
        let Ok(container) = systasis::systasis_container! {
            register_value!(value: [T; N] as IValue);
        }
        .build();
        assert_eq!(container.resolve_i_value(), value);
        assert_eq!(container.resolve_i_value(), value);
    }
}

mod borrowed {
    use super::*;
    #[systasis::container]
    pub fn run<'a, T: ?Sized>(value: &'a T)
    where
        &'a T: Copy,
    {
        let Ok(container) = systasis::systasis_container! {
            register_value!(value: &'a T as IValue);
        }
        .build();
        assert!(core::ptr::eq(container.resolve_i_value(), value));
    }
}

mod local {
    use super::*;
    #[systasis::container(require(!Sync))]
    pub fn run<T>(value: T) {
        let Ok(container) = systasis::systasis_container! {
            register_value!(value: T as IValue);
        }
        .build();
        let guard: core::cell::Ref<'_, T> = container.try_resolve_i_value_ref().unwrap();
        assert!(matches!(
            container.try_resolve_i_value(),
            Err(Error::ValueAccessContention)
        ));
        drop(guard);
        assert!(container.try_resolve_i_value().is_ok());
    }
}

mod unused {
    use super::*;
    #[systasis::container(require(Send, Sync))]
    pub fn run<T: ?Sized, const ENABLED: bool>() {
        let Ok(container) = systasis::systasis_container! {
            register_value!(17u32: u32 as IValue);
        }
        .build();
        assert_eq!(container.resolve_i_value(), 17);
    }
}

mod fresh {
    use super::*;
    #[systasis::container(require(Send, Sync))]
    pub fn run<T: Default>() {
        let Ok(container) = systasis::systasis_container! {
            register_type!(T as IValue);
        }
        .build();
        let _: T = container.resolve_i_value();
        let _: T = container.resolve_i_value();
    }
}

mod captured {
    use super::*;
    #[systasis::container]
    pub fn run<T: Clone + PartialEq + core::fmt::Debug>(value: T, expected: T) {
        let Ok(container) = systasis::systasis_container! {
            register_type_with!(T as IValue, || value.clone());
        }
        .build();
        assert_eq!(container.resolve_i_value(), expected);
        assert_eq!(container.resolve_i_value(), expected);
    }
}

mod constructor_dependency {
    use super::*;
    trait IStored {}
    impl<T> IStored for T {}
    #[systasis::container]
    pub fn run<T: Default + Clone>() -> Result<(), Error> {
        let container = systasis::systasis_container! {
            register_type_with!(T as IValue, || T::default());
            register_value!(resolve!(IValue): T as IStored);
        }
        .build::<Error>()?;
        let _: T = container.try_resolve_i_stored()?;
        let _: T = container.resolve_i_value();
        Ok(())
    }
}

#[test]
fn generic_value_policies_and_nameable_aliases() {
    unbounded::run(13u32, 13u32);
    unbounded::run(String::from("owned"), String::from("owned"));
    bounded::run(14u32);
    wrapper::run(wrapper::Wrapper(15u32));
    associated::run::<core::ops::Range<u32>>(16u32);
    array::run([1u32, 2, 3]);
    borrowed::run("borrowed");
    local::run(String::from("local"));
    unused::run::<std::rc::Rc<()>, true>();
}

#[test]
fn generic_fresh_and_captured_constructors() {
    fresh::run::<std::rc::Rc<()>>();
    captured::run(String::from("captured"), String::from("captured"));
    constructor_dependency::run::<String>().unwrap();
}
