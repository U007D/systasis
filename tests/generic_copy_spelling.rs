//! Equivalent grouping of an explicit whole-type bound preserves Copy access.
#![forbid(unsafe_code)]
#![allow(unused_parens)]

trait IValue {}
impl<T> IValue for T {}

mod parenthesized_registration {
    use super::IValue;

    fn inspect<T: Copy>(container: &AppContainer<T>) -> T {
        let _: &T = container.resolve_i_value_ref();
        let _: T = container.resolve_i_value();
        container.resolve_i_value()
    }

    #[systasis::container]
    pub fn run<T: Copy>(value: T) -> T {
        let Ok(container) = systasis::systasis_container! {
            register_value!(value: ((T)) as IValue);
        }
        .build();
        inspect(container)
    }
}

mod parenthesized_bound {
    use super::IValue;

    #[systasis::container]
    pub fn run<T>(value: T) -> T
    where
        (T): Copy,
    {
        let Ok(container) = systasis::systasis_container! {
            register_value!(value: T as IValue);
        }
        .build();
        let _: T = container.resolve_i_value();
        container.resolve_i_value()
    }
}

mod nested_parentheses {
    use super::IValue;

    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Wrapper<T>(pub T);

    #[systasis::container(require(Send, Sync))]
    pub fn run<T>(value: Wrapper<T>) -> Wrapper<T>
    where
        (Wrapper<(T)>): Copy + Send + Sync,
    {
        let Ok(container) = systasis::systasis_container! {
            register_value!(value: Wrapper<T> as IValue);
        }
        .build();
        let _: &Wrapper<T> = container.resolve_i_value_ref();
        let _: Wrapper<T> = container.resolve_i_value();
        container.resolve_i_value()
    }
}

#[test]
fn parenthesized_registration_uses_inline_bound_and_remains_nameable() {
    assert_eq!(parenthesized_registration::run(7u32), 7);
}

#[test]
fn parenthesized_where_bound_applies_to_the_registered_type() {
    assert_eq!(parenthesized_bound::run(8u32), 8);
}

#[test]
fn parentheses_inside_the_whole_type_bound_preserve_copy_access_and_auto_traits() {
    use nested_parentheses::Wrapper;
    assert_eq!(nested_parentheses::run(Wrapper(9u32)), Wrapper(9));
}
