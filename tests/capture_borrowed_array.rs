//! Explicit remainder annotations preserve captured borrowed array element lifetimes.
#![forbid(unsafe_code)]

trait ILength {}
impl ILength for usize {}

mod implied_bound {
    use super::ILength;
    type Input<'a, T> = [&'a T; 3];

    #[systasis::container(require(Send, Sync))]
    fn run<'a, T: Sync>(input: Input<'a, T>) {
        let [head, tail @ ..]: Input<'a, T> = input;
        let tail: [&'a T; 2] = tail;
        let Ok(container) = systasis::systasis_container! {
            register_type_with!(usize as ILength, move || {
                let exact: &[&'a T; 2] = &tail;
                exact.len()
            });
        }
        .build();
        let _: &'a T = head;
        assert_eq!(container.resolve_i_length(), 2);
        assert_eq!(container.resolve_i_length(), 2);
    }

    #[test]
    fn argument_implied_lifetime_is_preserved() {
        let values = [String::from("a"), String::from("b"), String::from("c")];
        run([&values[0], &values[1], &values[2]]);
    }
}

mod explicit_bound {
    use super::ILength;
    type Input<'a, T> = [&'a T; 3];

    #[systasis::container(require(Send, Sync))]
    fn run<'a, T: Sync + 'a>(input: Input<'a, T>) {
        let [head, tail @ ..]: Input<'a, T> = input;
        let tail: [&'a T; 2] = tail;
        let Ok(container) = systasis::systasis_container! {
            register_type_with!(usize as ILength, move || {
                let exact: &[&'a T; 2] = &tail;
                exact.len()
            });
        }
        .build();
        let _: &'a T = head;
        assert_eq!(container.resolve_i_length(), 2);
        assert_eq!(container.resolve_i_length(), 2);
    }

    #[test]
    fn explicit_lifetime_bound_remains_valid() {
        let values = [String::from("a"), String::from("b"), String::from("c")];
        run([&values[0], &values[1], &values[2]]);
    }
}
