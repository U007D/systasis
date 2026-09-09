//! Hoisting must preserve source-relative paths, including when names collide.
#![forbid(unsafe_code)]

#[derive(Clone, Copy)]
/// Parent-module implementation, distinct from the child's same-named type.
pub struct Value<T>(pub T);
/// Stored-value test interface.
pub trait IValue {}
impl<T> IValue for Value<T> {}
/// Fresh-constructor test interface.
pub trait IFresh {}
impl<T> IFresh for Value<T> {}
fn make(value: u32) -> Value<u32> {
    Value(value)
}

mod configured {
    // Deliberately different from the parent type with the same spelling.
    #[allow(dead_code)]
    struct Value;

    #[systasis::container]
    pub fn check<T>(value: T)
    where
        super::Value<T>: Copy,
    {
        let Ok(container) = systasis::systasis_container! {
            register_value!(super::Value(value): super::Value<T> as super::IValue);
            register_type_with!(super::Value<u32> as super::IFresh, || super::make(17));
        }
        .build();
        let _: super::Value<T> = container.resolve_i_value();
        assert_eq!(container.resolve_i_fresh().0, 17);
    }
}

#[test]
fn parent_paths_preserve_types_bounds_and_constructor_calls() {
    configured::check(7u32);
}
