//! Generic declarations retain their registration-site API after instantiation.
#![forbid(unsafe_code)]

use systasis::container::Error;

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
        let Ok(copied) = container.try_resolve_i_value();
        assert_eq!(copied, value);
        super::named_parameter(&container, value);
    }
}

fn named_parameter<T: Copy + PartialEq + core::fmt::Debug>(
    container: &bounded::SystasisContainer<T>,
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
        let _owned: T = container.try_resolve_i_value().unwrap();
        assert!(matches!(
            container.try_resolve_i_value(),
            Err(Error::ValueAlreadyConsumed)
        ));
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
        let Ok(_stored) = container.try_resolve_clone_i_stored();
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
    fallible::run::<String, &'static str>(Ok(String::from("ok")));
    captured_reference::run("captured borrow");
    returned_owned::run(String::from("owned")).unwrap();
    generic_interfaces::run(42u32);
    generic_dyn::run(String::from("dynamic"));
}

mod fallible {
    use super::*;
    #[systasis::container]
    pub fn run<T: Clone, E: Clone>(value: Result<T, E>) {
        let Ok(container) = systasis::systasis_container! {
            register_type_with!(T as IValue, try || -> Result<T, E> { value.clone() });
        }
        .build();
        let _: Result<T, E> = container.try_resolve_i_value();
        let _: Result<T, E> = container.try_resolve_i_value();
    }
}

mod captured_reference {
    use super::*;
    #[systasis::container]
    pub fn run<'a>(value: &'a str) {
        let Ok(container) = systasis::systasis_container! {
            register_type_with!(&'a str as IValue, || value);
        }
        .build();
        assert_eq!(container.resolve_i_value(), value);
    }
}

mod returned_owned {
    use super::*;
    trait IOutput {}
    impl<T> IOutput for T {}
    #[systasis::container]
    pub fn run<T>(value: T) -> Result<(), Error> {
        let container = systasis::systasis_container! {
            register_value!(value: T as IValue);
            register_type_with!(T as IOutput, try || -> Result<T, Error> { try_resolve!(IValue) });
        }
        .build::<Error>()?;
        let _owned = container.try_resolve_i_output()?;
        assert!(matches!(
            container.try_resolve_i_value(),
            Err(Error::ValueAlreadyConsumed)
        ));
        Ok(())
    }
}

mod generic_interfaces {
    trait IFirst<T> {}
    trait ISecond<T> {}
    impl<T> IFirst<T> for T {}
    impl<T> ISecond<T> for T {}
    #[systasis::container]
    pub fn run<T: Copy>(value: T) {
        let Ok(container) = systasis::systasis_container! {
            register_value!(value: T as IFirst<T> + ISecond<T>);
        }
        .build();
        let _: T = container.resolve_i_first_i_second();
    }
}

mod generic_dyn {
    trait IValue<T> {
        fn get(&self) -> &T;
    }
    impl<T> IValue<T> for T {
        fn get(&self) -> &T {
            self
        }
    }
    #[systasis::container]
    pub fn run<T>(value: T) {
        let Ok(container) = systasis::systasis_container! {
            register_value!(value: T as dyn IValue<T>);
        }
        .build();
        let value = container.try_resolve_i_value().unwrap();
        let object: &dyn IValue<T> = &value;
        let _: &T = object.get();
    }
}
