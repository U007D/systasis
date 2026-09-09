//! Named registrations preserve independent values, policies and dependencies.
#![forbid(unsafe_code)]

use systasis::app_container::Error;

trait IValue {}
impl<T> IValue for T {}
trait ISize {}
impl ISize for usize {}

#[systasis::container]
#[test]
fn defaults_named_values_and_copy_queries() {
    let Ok(container) = systasis::systasis_container! {
        register_value!(10u32: u32 as IValue);
        register_value!(20u32: u32 as IValue in test);
        register_value!(resolve_from!(IValue, test) + resolve_from!(IValue, default): resolve_type_from!(IValue, test) as IValue in sum);
        register_value!(*resolve_ref_from!(IValue, test): u32 as IValue in shared);
        register_value!(resolve_clone_from!(IValue, test): u32 as IValue in cloned);
    }.build();
    assert_eq!(container.resolve_i_value(), 10);
    assert_eq!(container.resolve_i_value_in_default(), 10);
    assert_eq!(*container.resolve_i_value_ref_in_default(), 10);
    assert_eq!(container.resolve_i_value_clone_in_default(), 10);
    assert_eq!(container.resolve_i_value_in_test(), 20);
    assert_eq!(*container.resolve_i_value_ref_in_test(), 20);
    assert_eq!(container.resolve_i_value_clone_in_test(), 20);
    assert_eq!(container.resolve_i_value_in_sum(), 30);
    assert_eq!(container.resolve_i_value_in_shared(), 20);
    assert_eq!(container.resolve_i_value_in_cloned(), 20);
}

mod owned {
    use super::*;
    #[systasis::container]
    #[test]
    fn namespaces_keep_borrowing_consumption_and_owned_dependencies_separate() -> Result<(), Error>
    {
        let container = systasis::systasis_container! {
            register_value!(try_resolve_from!(IValue, source)?: resolve_type_from!(IValue, source) as IValue in destination);
            register_value!(String::from("source"): String as IValue in source);
            register_value!(String::from("untouched"): String as IValue);
        }.build::<Error>()?;
        assert!(matches!(
            container.try_resolve_i_value_in_source(),
            Err(Error::ValueAlreadyConsumed)
        ));
        let guard = container.try_resolve_i_value_ref_in_destination()?;
        assert_eq!(&*guard, "source");
        assert!(matches!(
            container.try_resolve_i_value_ref_mut_in_destination(),
            Err(Error::ValueAccessContention)
        ));
        drop(guard);
        container
            .try_resolve_i_value_ref_mut_in_destination()?
            .push('!');
        assert_eq!(
            container.try_resolve_i_value_clone_in_destination()?,
            "source!"
        );
        assert_eq!(container.try_resolve_i_value_in_destination()?, "source!");
        assert_eq!(container.try_resolve_i_value_in_default()?, "untouched");
        Ok(())
    }
}

mod constructors {
    use super::*;
    #[systasis::container(require(!Sync))]
    #[test]
    fn named_fresh_and_captured_constructors_use_named_guard_queries() -> Result<(), Error> {
        let suffix: String = String::from("!");
        let container = systasis::systasis_container! {
            register_value!(String::from("name"): String as IValue in data);
            register_type!(String as IValue in fresh);
            register_type_with!(String as IValue in capture, || {
                let mut value = resolve_from!(IValue, fresh);
                value.push_str(&suffix);
                value
            });
            register_type_with!(usize as ISize in shared, try || -> Result<usize, Error> {
                Ok(try_resolve_ref_from!(IValue, data)?.len())
            });
            register_type_with!(usize as ISize in mutate, try || -> Result<usize, Error> {
                let mut value = try_resolve_ref_mut_from!(IValue, data)?;
                value.push('!');
                Ok(value.len())
            });
            register_type_with!(String as IValue in cloned, try || -> Result<String, Error> {
                try_resolve_clone_from!(IValue, data)
            });
        }
        .build::<Error>()?;
        assert_eq!(container.resolve_i_value_in_capture(), "!");
        assert_eq!(container.resolve_i_value_in_capture(), "!");
        assert_eq!(container.try_resolve_i_size_in_shared()?, 4);
        assert_eq!(container.try_resolve_i_size_in_mutate()?, 5);
        assert_eq!(container.try_resolve_i_value_in_cloned()?, "name!");
        Ok(())
    }
}

mod overrides {
    use super::*;
    #[systasis::container]
    #[test]
    fn last_winner_is_selected_per_namespace_and_explicit_default_is_equivalent() {
        let discarded: String = String::from("still owned");
        let Ok(container) = systasis::systasis_container! {
            register_value!(missing!(): MissingType as IValue);
            register_value!(11u32: u32 as IValue in default);
            register_type_with!(String as IValue in named, || discarded.clone());
            register_value!(22u32: u32 as IValue in named);
            register_value!(33u32: u32 as IValue in r#type);
        }
        .build();
        assert_eq!(container.resolve_i_value(), 11);
        assert_eq!(container.resolve_i_value_in_named(), 22);
        assert_eq!(container.resolve_i_value_in_type(), 33);
        assert_eq!(discarded, "still owned");
    }
}

mod dyn_group {
    use super::*;
    trait IRead {
        fn text(&self) -> &str;
    }
    trait ILength {
        fn length(&self) -> usize;
    }
    impl IRead for String {
        fn text(&self) -> &str {
            self
        }
    }
    impl ILength for String {
        fn length(&self) -> usize {
            self.len()
        }
    }
    #[systasis::container]
    #[test]
    fn named_dyn_group_queries_and_type_lookup_keep_the_complete_identity() -> Result<(), Error> {
        let container = systasis::systasis_container! {
            register_value!(String::from("dyn"): String as dyn IRead + ILength in data);
            register_value!({
                let guard = try_resolve_dyn_ref_from!(ILength + IRead, data)?;
                let object: &resolve_type_from!(dyn IRead + ILength, data) = &*guard;
                object.length()
            }: usize as ISize);
        }
        .build::<Error>()?;
        assert_eq!(container.resolve_i_size(), 3);
        let guard = container.try_resolve_i_length_i_read_dyn_ref_in_data()?;
        assert_eq!(guard.text(), "dyn");
        Ok(())
    }
}

mod generic {
    use super::*;
    #[systasis::container]
    pub fn run<T: Copy + PartialEq + core::fmt::Debug>(first: T, second: T) {
        let Ok(container) = systasis::systasis_container! {
            register_value!(first: T as IValue);
            register_value!(second: T as IValue in named);
            register_type_with!(T as IValue in derived, || resolve_from!(IValue, named));
            register_type_with!(T as IValue in from_default, || resolve!(IValue));
        }
        .build();
        assert_eq!(container.resolve_i_value(), first);
        assert_eq!(container.resolve_i_value_in_named(), second);
        assert_eq!(container.resolve_i_value_in_derived(), second);
        assert_eq!(container.resolve_i_value_in_from_default(), first);
    }
}

#[test]
fn generic_namespace_policies_remain_bound_based() {
    generic::run(1u32, 2u32);
}

mod copy_dyn {
    use super::*;
    trait INumber {
        fn number(&self) -> u32;
    }
    impl INumber for u32 {
        fn number(&self) -> u32 {
            *self
        }
    }
    #[systasis::container]
    #[test]
    fn named_copy_dyn_query_returns_plain_reference() {
        let Ok(container) = systasis::systasis_container! {
            register_value!(41u32: u32 as dyn INumber in named);
            register_value!({
                let object: &resolve_type_from!(dyn INumber, named) = resolve_dyn_ref_from!(INumber, named);
                object.number() + 1
            }: u32 as IValue);
        }.build();
        assert_eq!(container.resolve_i_number_dyn_ref_in_named().number(), 41);
        assert_eq!(container.resolve_i_value(), 42);
    }
}
mod raw_interface {
    trait IValue {}
    impl IValue for u32 {}

    #[systasis::container]
    #[test]
    fn raw_interface_spelling_does_not_change_identity() {
        let Ok(container) = systasis::systasis_container! {
            register_value!(1: u32 as r#IValue in r#source);
            register_value!(resolve_from!(IValue, source) + 1: resolve_type_from!(r#IValue, source) as IValue);
        }.build();
        assert_eq!(container.resolve_i_value(), 2);
    }
}
