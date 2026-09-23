//! Declared value types retain Rust's coercion context, including mutable references.
#![forbid(unsafe_code)]

trait Number {
    fn number(&self) -> i16;
}
impl Number for u8 {
    fn number(&self) -> i16 {
        i16::from(*self)
    }
}
impl Number for i8 {
    fn number(&self) -> i16 {
        i16::from(*self)
    }
}
trait IMutable {}
impl<T: ?Sized> IMutable for &mut T {}

mod shared {
    use super::*;
    #[systasis::container]
    fn init<'a>(unsigned: &'a u8, signed: &'a i8, choose: bool) -> SystasisContainer<'a> {
        let Ok(container) = systasis::systasis_container! {
            register_value!(if choose { unsigned } else { signed }: &'a dyn Number as Copy);
            register_value!(match choose { true => unsigned, false => signed }: &'a dyn Number as Copy in matched);
        }.build();
        container
    }
    #[test]
    fn annotation_coerces_heterogeneous_shared_branches() {
        for (choose, expected) in [(true, 7), (false, -3)] {
            let container = init(&7, &-3, choose);
            assert_eq!(container.resolve_copy().number(), expected);
            let Ok(value) = container.try_resolve_copy_in_matched();
            assert_eq!(value.number(), expected);
        }
    }
}

mod exclusive {
    use super::*;
    #[systasis::container]
    fn init<'a>(unsigned: &'a mut u8, signed: &'a mut i8, choose: bool) -> SystasisContainer<'a> {
        let Ok(container) = systasis::systasis_container! {
            register_value!(if choose { unsigned } else { signed }: &'a mut dyn Number as IMutable);
        }
        .build();
        container
    }
    #[test]
    fn annotation_coerces_mutable_branches_without_lending_from_container() {
        for (choose, expected) in [(true, 7), (false, -3)] {
            let mut unsigned = 7;
            let mut signed = -3;
            let value = {
                let container = init(&mut unsigned, &mut signed, choose);
                let value = container.try_resolve_i_mutable().unwrap();
                assert!(container.try_resolve_i_mutable().is_err());
                value
            };
            assert_eq!(value.number(), expected);
        }
    }
}

mod slice {
    use super::*;
    type Mutable<'a> = &'a mut [u8];
    #[systasis::container]
    fn init<'a>(input: &'a mut [u8; 2]) -> SystasisContainer<'a> {
        let builder = systasis::systasis_container! {
            register_value!(input: Mutable<'a> as IMutable);
        };
        let Ok(container) = builder.build();
        container
    }
    #[test]
    fn mutable_array_unsizing_preserves_declared_reference_lifetime() {
        let mut input = [1, 2];
        let slice = { init(&mut input).try_resolve_i_mutable().unwrap() };
        slice[1] = 3;
        assert_eq!(input, [1, 3]);
    }
}
