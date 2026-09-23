//! A macro-free constructor transfers a stored mutable reference from a named child.
#![forbid(unsafe_code)]

use systasis::container::Error;

struct Borrowed<'env>(&'env str);
trait IValue {}
impl IValue for &mut Borrowed<'_> {}

mod child {
    use super::*;

    #[systasis::container]
    pub fn run<'loan, 'env>(
        value: &'loan mut Borrowed<'env>,
        call: impl FnOnce(&SystasisContainer<'loan, 'env>),
    ) {
        let Ok(container) = systasis::systasis_container! {
            register_value!(value: &'loan mut Borrowed<'env> as IValue);
        }
        .build();
        call(&container);
    }
}

mod parent {
    use super::*;

    struct Editor<'loan, 'env>(&'loan mut Borrowed<'env>);
    trait IEditor {}
    impl IEditor for Editor<'_, '_> {}

    #[systasis::container(require(Send, Sync))]
    fn check<'loan, 'env>(primary: &child::SystasisContainer<'loan, 'env>, replacement: &'env str) {
        let Ok(container) = systasis::systasis_container! {
        register_container!(primary: &child::SystasisContainer<'loan, 'env>);
        register_type_with!(Editor<'loan, 'env> as IEditor, try || -> Result<Editor<'loan, 'env>, Error> {
            let value = try_resolve_from!(IValue, primary)?;
            Ok(Editor(value))
        });
    }
    .build();

        let editor = container.try_resolve_i_editor().unwrap();
        // Keep assertions outside the constructor so it uses no caller macros.
        assert_eq!(editor.0.0, "before");
        assert!(matches!(
            primary.try_resolve_i_value(),
            Err(Error::ValueAlreadyConsumed)
        ));
        assert!(matches!(
            container.try_resolve_i_editor(),
            Err(Error::ValueAlreadyConsumed)
        ));
        editor.0.0 = replacement;

        std::thread::scope(|scope| {
            scope
                .spawn(move || assert_eq!(editor.0.0, replacement))
                .join()
                .unwrap();
        });
        assert!(matches!(
            container.try_resolve_i_editor(),
            Err(Error::ValueAlreadyConsumed)
        ));
    }

    #[test]
    fn child_mutable_reference_transfers_once_and_preserves_mutation_and_send() {
        let initial = String::from("before");
        let replacement = String::from("after");
        let mut value = Borrowed(&initial);
        child::run(&mut value, |primary| check(primary, &replacement));
        assert_eq!(value.0, replacement);
    }
}
