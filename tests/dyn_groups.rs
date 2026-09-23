//! Explicit group targets retain concrete storage and expose all opted-in traits.
#![forbid(unsafe_code)]

use systasis::container::Error;

trait IReader {
    type Item;
    fn read(&self) -> Self::Item;
}
trait IWriter {
    fn length(&self) -> usize;
}
impl IReader for String {
    type Item = usize;
    fn read(&self) -> usize {
        self.len()
    }
}
impl IWriter for String {
    fn length(&self) -> usize {
        self.len()
    }
}
impl IReader for u32 {
    type Item = usize;
    fn read(&self) -> usize {
        *self as usize
    }
}
impl IWriter for u32 {
    fn length(&self) -> usize {
        *self as usize
    }
}
trait IOutput {}
impl IOutput for usize {}

macro_rules! scenario {
    ($module:ident, ($($requirements:tt)*)) => {
        mod $module {
            use super::*;
            #[systasis::container($($requirements)*)]
            #[test]
            fn cloned_group_value_supports_explicit_trait_object_borrows() -> Result<(), Error> {
                let container = systasis::systasis_container! {
                    register_value!(String::from("group"): String as dyn IWriter + IReader<Item = usize>);
                }.build::<Error>()?;
                let mut owned = container.resolve_i_reader_i_writer_clone();
                let reader: &dyn IReader<Item = usize> = &owned;
                let writer: &dyn IWriter = &owned;
                assert_eq!(reader.read(), 5);
                assert_eq!(writer.length(), 5);
                owned.push('!');
                let Ok(another) = container.try_resolve_i_reader_i_writer_clone();
                assert_eq!(another, "group");
                Ok(())
            }
        }
    };
}
scenario!(synchronized, (require(Send, Sync)));
scenario!(local, (require(!Sync)));

mod copy {
    use super::*;
    #[systasis::container]
    #[test]
    fn copy_dyn_queries_and_type_lookup_keep_static_accessors() {
        let Ok(container) = systasis::systasis_container! {
            register_value!({
                let copied = resolve!(IReader<Item = usize> + IWriter);
                let value: &resolve_type!(dyn IWriter + IReader<Item = usize>) = &copied;
                value.read() + value.length()
            }: usize as IOutput);
            register_value!(12: u32 as dyn IReader<Item = usize> + IWriter);
        }
        .build();
        assert_eq!(container.resolve_i_output(), 24);
        assert_eq!(container.resolve_i_reader_i_writer(), 12);
        let copied = container.resolve_i_reader_i_writer();
        let reader: &dyn IReader<Item = usize> = &copied;
        assert_eq!(reader.read(), 12);
    }
}

mod constructor {
    use super::*;
    #[systasis::container]
    #[test]
    fn constructor_dyn_query_uses_the_same_normalized_target() -> Result<(), Error> {
        let container = systasis::systasis_container! {
            register_type_with!(usize as IOutput, try || -> Result<usize, Error> {
                let value = resolve_clone!(IWriter + IReader<Item = usize>);
                let typed: &resolve_type!(dyn IReader<Item = usize> + IWriter) = &value;
                Ok(typed.read() + typed.length())
            });
            register_value!(String::from("group"): String as dyn IReader<Item = usize> + IWriter);
        }
        .build::<Error>()?;
        assert_eq!(container.try_resolve_i_output()?, 10);
        assert_eq!(container.try_resolve_i_output()?, 10);
        Ok(())
    }
}

#[allow(clippy::multiple_bound_locations)] // Exercise both authored bound locations.
mod borrowed_generics {
    use super::*;
    trait IBorrow<'a> {
        type Item;
        fn item(&self) -> Self::Item;
    }
    trait ICount<T, const N: usize> {
        fn count(&self) -> usize;
    }
    #[derive(Clone)]
    struct Value<'a, T>(&'a T);
    impl<'a, T> IBorrow<'a> for Value<'a, T> {
        type Item = &'a T;
        fn item(&self) -> &'a T {
            self.0
        }
    }
    impl<T, const N: usize> ICount<T, N> for Value<'_, T> {
        fn count(&self) -> usize {
            N
        }
    }
    #[systasis::container]
    fn configure<'a, T: Clone + PartialEq, const N: usize>(input: &'a T) -> Result<(), Error>
    where
        T: 'a,
        Value<'a, T>: Clone,
    {
        let container = systasis::systasis_container! {
            register_type_with!(usize as IOutput, try || -> Result<usize, Error> {
                let value = resolve_clone!(IBorrow<'a, Item = &'a T> + ICount<T, N>);
                let typed: &resolve_type!(dyn ICount<T, N> + IBorrow<'a, Item = &'a T>) = &value;
                Ok(typed.count())
            });
            register_value!(Value(input): Value<'a, T> as dyn ICount<T, N> + IBorrow<'a, Item = &'a T>);
        }.build::<Error>()?;
        let value = container.resolve_i_borrow_i_count_clone();
        let borrowed: &dyn IBorrow<'a, Item = &'a T> = &value;
        let count: &dyn ICount<T, N> = &value;
        assert!(borrowed.item() == input);
        assert_eq!(count.count(), N);
        assert_eq!(container.try_resolve_i_output()?, N);
        Ok(())
    }
    #[test]
    fn associated_types_and_borrowed_generic_lifetimes_are_preserved() -> Result<(), Error> {
        let input = String::from("nonstatic");
        configure::<String, 7>(&input)
    }
}

mod independent_lifetimes {
    use super::*;
    trait ILong<'long> {
        fn long(&self) -> &'long str;
    }
    trait IShort {
        fn short(&self) -> &str;
    }
    struct Value<'short, 'long> {
        short: &'short str,
        long: &'long str,
    }
    impl<'long> ILong<'long> for Value<'_, 'long> {
        fn long(&self) -> &'long str {
            self.long
        }
    }
    impl IShort for Value<'_, '_> {
        fn short(&self) -> &str {
            self.short
        }
    }
    #[systasis::container(require(!Sync))]
    fn configure<'short, 'long>(short: &'short str, long: &'long str) -> Result<(), Error> {
        let container = systasis::systasis_container! {
            register_value!(Value { short, long }: Value<'short, 'long> as dyn ILong<'long> + IShort);
        }.build::<Error>()?;
        let value = container.try_resolve_i_long_i_short()?;
        let long_view: &dyn ILong<'long> = &value;
        let short_view: &dyn IShort = &value;
        assert_eq!(long_view.long(), long);
        assert_eq!(short_view.short(), short);
        Ok(())
    }
    #[test]
    fn implementor_need_not_outlive_every_trait_lifetime_argument() -> Result<(), Error> {
        let long = String::from("long");
        {
            let short = String::from("short");
            configure(&short, &long)?;
        }
        assert_eq!(long, "long");
        Ok(())
    }
}

mod returned_owned {
    use super::*;
    struct View<G>(G);
    trait IView {}
    impl<G> IView for View<G> {}
    #[systasis::container]
    #[test]
    fn custom_constructor_can_return_an_owned_combined_target() -> Result<(), Error> {
        let container = systasis::systasis_container! {
            register_type_with!(View<resolve_type!(IReader<Item = usize> + IWriter)> as IView,
                try || -> Result<View<resolve_type!(IWriter + IReader<Item = usize>)>, Error> {
                    let owned = resolve_clone!(IReader<Item = usize> + IWriter);
                    let target: &resolve_type!(dyn IWriter + IReader<Item = usize>) = &owned;
                    assert_eq!(target.read(), target.length());
                    Ok(View(owned))
                });
            register_value!(String::from("retained"): String as dyn IWriter + IReader<Item = usize>);
        }.build::<Error>()?;
        let mut view = container.try_resolve_i_view()?;
        assert_eq!(view.0.read(), 8);
        assert_eq!(view.0.length(), 8);
        view.0.push('!');
        assert_eq!(view.0.length(), 9);
        assert_eq!(container.try_resolve_i_view()?.0.length(), 8);
        Ok(())
    }
}
