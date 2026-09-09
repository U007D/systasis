//! Structurally annotated destructuring preserves Rust's implicit reference bindings.
#![forbid(unsafe_code)]

trait IText {}
impl IText for String {}
trait INumber {}
impl INumber for u32 {}

mod shared_parameter {
    use super::*;

    #[systasis::container]
    fn run((text, number): &(String, u32)) {
        let Ok(container) = systasis::systasis_container! {
            register_type_with!(String as IText, move || text.clone());
            register_type_with!(u32 as INumber, move || *number);
        }
        .build();
        assert_eq!(container.resolve_i_text(), "shared");
        assert_eq!(container.resolve_i_text(), "shared");
        assert_eq!(container.resolve_i_number(), 7);
    }

    #[test]
    fn implicitly_borrowed_parameter_fields_remain_references() {
        let input = (String::from("shared"), 7);
        run(&input);
        assert_eq!(input.0, "shared");
    }
}

mod nested_local {
    use super::*;

    #[systasis::container]
    fn run(input: &(String, [u32; 3])) {
        let (text, [first, _, last]): &(String, [u32; 3]) = input;
        let Ok(container) = systasis::systasis_container! {
            register_type_with!(String as IText, move || text.clone());
            register_type_with!(u32 as INumber, move || *first + *last);
        }
        .build();
        assert_eq!(container.resolve_i_text(), "nested");
        assert_eq!(container.resolve_i_number(), 10);
    }

    #[test]
    fn nested_tuple_and_array_bindings_keep_inherited_shared_borrows() {
        let input = (String::from("nested"), [3, 5, 7]);
        run(&input);
        assert_eq!(input.1, [3, 5, 7]);
    }
}

mod mutable_parameter {
    use super::*;

    #[systasis::container]
    fn run((text, number): &mut (String, u32)) {
        let Ok(container) = systasis::systasis_container! {
            register_type_with!(String as IText, move || text.clone());
            register_type_with!(u32 as INumber, move || *number);
        }
        .build();
        assert_eq!(container.resolve_i_text(), "exclusive");
        assert_eq!(container.resolve_i_number(), 12);
    }

    #[test]
    fn owned_mutable_reference_captures_are_released_with_container() {
        let mut input = (String::from("exclusive"), 12);
        run(&mut input);
        input.0.push('!');
        input.1 += 1;
        assert_eq!(input, (String::from("exclusive!"), 13));
    }
}

mod mixed_references {
    use super::*;

    #[systasis::container]
    fn run((number,): &&mut (u32,)) {
        let Ok(container) = systasis::systasis_container! {
            register_type_with!(u32 as INumber, move || *number);
        }
        .build();
        assert_eq!(container.resolve_i_number(), 21);
        assert_eq!(container.resolve_i_number(), 21);
    }

    #[test]
    fn shared_reference_prevents_promoting_inner_mutable_reference_to_exclusive_binding() {
        let mut input = (21,);
        run(&&mut input);
        input.0 += 1;
        assert_eq!(input.0, 22);
    }
}

mod returned_reference {
    trait IView {}
    impl IView for &str {}

    #[systasis::container]
    fn run((text,): &(String,)) {
        let Ok(container) = systasis::systasis_container! {
            register_type_with!(&'_ str as IView, move || text.as_str());
        }
        .build();
        let first = container.resolve_i_view();
        let second = container.resolve_i_view();
        assert_eq!(first, "borrowed output");
        assert!(core::ptr::eq(first.as_ptr(), text.as_ptr()));
        assert!(core::ptr::eq(first.as_ptr(), second.as_ptr()));
    }

    #[test]
    fn returned_references_preserve_the_destructured_capture_borrow() {
        let input = (String::from("borrowed output"),);
        run(&input);
        assert_eq!(input.0, "borrowed output");
    }
}
