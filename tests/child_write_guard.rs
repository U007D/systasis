//! A macro-free constructor can return a Send write guard from a named child.
#![forbid(unsafe_code)]

use systasis::app_container::Error;

struct Borrowed<'env>(&'env str);
trait IValue {}
impl IValue for Borrowed<'_> {}

mod child {
    use super::*;

    #[systasis::container]
    pub fn run<'env>(value: &'env str, call: impl FnOnce(&AppContainer<'env>)) {
        let Ok(container) = systasis::systasis_container! {
            register_value!(Borrowed(value): Borrowed<'env> as IValue);
        }
        .build();
        call(&container);
    }
}

mod parent {
    use super::*;

    struct Editor<'a, 'env>(systasis::RefMut<'a, Borrowed<'env>>);
    trait IEditor {}
    impl IEditor for Editor<'_, '_> {}

    #[systasis::container(require(Send, Sync))]
    fn check<'env>(primary: &child::AppContainer<'env>, replacement: &'env str) {
        let Ok(container) = systasis::systasis_container! {
        register_container!(primary: &child::AppContainer<'env>);
        register_type_with!(Editor<'_, 'env> as IEditor, try || -> Result<Editor<'_, 'env>, Error> {
            let value = try_resolve_ref_mut_from!(IValue, primary)?;
            Ok(Editor(value))
        });
    }
    .build();

        let mut editor = container.try_resolve_i_editor().unwrap();
        // Keep assertions outside the constructor so it uses no caller macros.
        assert_eq!(editor.0.0, "before");
        assert!(matches!(
            primary.try_resolve_i_value_ref(),
            Err(Error::ValueAccessContention)
        ));
        assert!(matches!(
            container.try_resolve_i_editor(),
            Err(Error::ValueAccessContention)
        ));
        editor.0.0 = replacement;

        std::thread::scope(|scope| scope.spawn(move || drop(editor)).join().unwrap());
        assert_eq!(primary.try_resolve_i_value_ref().unwrap().0, replacement);
        assert_eq!(container.try_resolve_i_editor().unwrap().0.0, replacement);
    }

    #[test]
    fn child_write_guard_preserves_mutation_contention_and_send() {
        let initial = String::from("before");
        let replacement = String::from("after");
        child::run(&initial, |primary| check(primary, &replacement));
    }
}
