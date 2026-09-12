//! Child data lifetimes must not become static requirements for native closures.
#![forbid(unsafe_code)]
use systasis::app_container::Error;
/// Non-Copy payload retaining an external reference.
struct Borrowed<'a>(&'a str);
trait IValue {}
impl IValue for Borrowed<'_> {}
struct View<'a, 'env>(systasis::Ref<'a, Borrowed<'env>>);
trait IView {}
impl IView for View<'_, '_> {}
mod leaf {
    use super::*;
    #[systasis::container]
    pub fn run<'env>(value: &'env str, call: impl FnOnce(&AppContainer<'env>)) {
        let Ok(container) = systasis::systasis_container! {
            register_value!(Borrowed(value): Borrowed<'env> as IValue);
        }
        .build();
        call(container);
    }
}
mod single {
    use super::*;
    #[systasis::container]
    fn check<'env>(primary: &leaf::AppContainer<'env>) {
        let Ok(container) = systasis::systasis_container! {
            register_container!(primary: &leaf::AppContainer<'env>);
            register_type_with!(View<'_, 'env> as IView, try || -> Result<View<'_, 'env>, Error> {
                let value = try_resolve_ref_from!(IValue, primary)?;
                assert!(!value.0.is_empty());
                Ok(View(value))
            });
        }
        .build();
        let view = container.try_resolve_i_view().unwrap();
        assert_eq!(view.0.0, "borrowed");
        assert!(matches!(
            primary.try_resolve_i_value_ref_mut(),
            Err(Error::ValueAccessContention)
        ));
        drop(view);
        assert!(primary.try_resolve_i_value_ref_mut().is_ok());
    }
    #[test]
    fn invariant_child_payload_keeps_its_nonstatic_lifetime() {
        let value = String::from("borrowed");
        leaf::run(value.as_str(), check);
    }
}

mod multiple {
    use super::*;
    struct Pair<'a, 'left, 'right>(
        systasis::Ref<'a, Borrowed<'left>>,
        systasis::Ref<'a, Borrowed<'right>>,
    );
    trait IPair {}
    impl IPair for Pair<'_, '_, '_> {}
    #[systasis::container]
    fn check<'left, 'right>(
        primary: &leaf::AppContainer<'left>,
        replica: &leaf::AppContainer<'right>,
    ) {
        let Ok(container) = systasis::systasis_container! {
            register_container!(primary: &leaf::AppContainer<'left>);
            register_container!(replica: &leaf::AppContainer<'right>);
            register_type_with!(Pair<'_, 'left, 'right> as IPair, try || -> Result<Pair<'_, 'left, 'right>, Error> {
                let first = try_resolve_ref_from!(IValue, primary)?;
                let second = try_resolve_ref_from!(IValue, replica)?;
                assert_eq!(first.0, second.0);
                Ok(Pair(first, second))
            });
        }
        .build();
        let pair = container.try_resolve_i_pair().unwrap();
        assert_eq!(pair.0.0, pair.1.0);
        assert!(matches!(
            primary.try_resolve_i_value_ref_mut(),
            Err(Error::ValueAccessContention)
        ));
        assert!(matches!(
            replica.try_resolve_i_value_ref_mut(),
            Err(Error::ValueAccessContention)
        ));
        drop(pair);
        assert!(primary.try_resolve_i_value_ref_mut().is_ok());
        assert!(replica.try_resolve_i_value_ref_mut().is_ok());
    }
    #[test]
    fn two_independently_owned_children_retain_both_guards() {
        let left = String::from("both");
        leaf::run(&left, |primary| {
            let right = String::from("both");
            leaf::run(&right, |replica| check(primary, replica))
        });
    }
}

mod explicit_child_lifetime {
    use super::*;

    #[systasis::container]
    fn check<'a, 'env>(primary: &'a leaf::AppContainer<'env>) {
        let Ok(container) = systasis::systasis_container! {
            register_container!(primary: &'a leaf::AppContainer<'env>);
            register_type_with!(View<'_, 'env> as IView, try || -> Result<View<'_, 'env>, Error> {
                let value = try_resolve_ref_from!(IValue, primary)?;
                assert!(!value.0.is_empty());
                Ok(View(value))
            });
        }
        .build();
        assert_eq!(container.try_resolve_i_view().unwrap().0.0, "explicit");
    }

    #[test]
    fn authored_outer_child_lifetime_can_outlive_one_resolution() {
        let value = String::from("explicit");
        leaf::run(&value, check);
    }
}

mod nested {
    use systasis::app_container::Error;
    trait IValue {}
    impl IValue for String {}
    struct View<'a>(systasis::Ref<'a, String>);
    trait IView {}
    impl IView for View<'_> {}
    mod leaf {
        use super::*;
        #[systasis::container]
        pub fn run(value: String, call: impl FnOnce(&AppContainer)) {
            let Ok(container) = systasis::systasis_container! {
                register_value!(value: String as IValue);
            }
            .build();
            call(container);
        }
    }
    mod middle {
        use super::*;
        #[systasis::container]
        pub fn run<'a>(primary: &'a leaf::AppContainer, call: impl FnOnce(&AppContainer<'a>)) {
            let Ok(container) = systasis::systasis_container! {
                register_container!(primary: &'a leaf::AppContainer);
            }
            .build();
            call(container);
        }
    }
    #[systasis::container]
    fn check<'a>(branch: &middle::AppContainer<'a>) {
        let Ok(container) = systasis::systasis_container! {
            register_container!(branch: &middle::AppContainer<'a>);
            register_type_with!(View<'_> as IView, try || -> Result<View<'_>, Error> {
                let value = try_resolve_ref_from!(IValue, branch::primary)?;
                assert!(!value.is_empty());
                Ok(View(value))
            });
        }
        .build();
        let view = container.try_resolve_i_view().unwrap();
        assert_eq!(&*view.0, "nested");
        assert!(matches!(
            branch.primary().try_resolve_i_value_ref_mut(),
            Err(Error::ValueAccessContention)
        ));
        drop(view);
        assert!(branch.primary().try_resolve_i_value_ref_mut().is_ok());
    }
    #[test]
    fn nested_child_context_retains_guard_after_temporary_descriptor_drops() {
        let value = String::from("nested");
        leaf::run(value, |primary| middle::run(primary, check));
    }
}

mod exclusive {
    use super::*;

    struct Editor<'a, 'env>(systasis::RefMut<'a, Borrowed<'env>>);
    trait IEditor {}
    impl IEditor for Editor<'_, '_> {}

    #[systasis::container(require(Send, Sync))]
    fn check<'env>(primary: &leaf::AppContainer<'env>, replacement: &'env str) {
        let Ok(container) = systasis::systasis_container! {
            register_container!(primary: &leaf::AppContainer<'env>);
            register_type_with!(Editor<'_, 'env> as IEditor, try || -> Result<Editor<'_, 'env>, Error> {
                let value = try_resolve_ref_mut_from!(IValue, primary)?;
                assert!(!value.0.is_empty());
                Ok(Editor(value))
            });
        }
        .build();
        let mut editor = container.try_resolve_i_editor().unwrap();
        assert!(matches!(
            primary.try_resolve_i_value_ref(),
            Err(Error::ValueAccessContention)
        ));
        editor.0.0 = replacement;
        // The synchronized guard may be dropped on a different thread.
        std::thread::scope(|scope| scope.spawn(move || drop(editor)).join().unwrap());
        assert_eq!(primary.try_resolve_i_value_ref().unwrap().0, replacement);
    }

    #[test]
    fn native_child_write_guard_preserves_mutation_and_send() {
        let initial = String::from("before");
        let replacement = String::from("after");
        leaf::run(&initial, |primary| check(primary, &replacement));
    }
}

mod local {
    use super::*;

    struct Editor<'a, 'env>(core::cell::RefMut<'a, Borrowed<'env>>);
    trait IEditor {}
    impl IEditor for Editor<'_, '_> {}

    mod leaf {
        use super::*;

        #[systasis::container(require(!Sync))]
        pub fn run<'env>(value: &'env str, call: impl FnOnce(&AppContainer<'env>)) {
            let Ok(container) = systasis::systasis_container! {
                register_value!(Borrowed(value): Borrowed<'env> as IValue);
            }
            .build();
            call(container);
        }
    }

    #[systasis::container(require(!Sync))]
    fn check<'env>(primary: &leaf::AppContainer<'env>, replacement: &'env str) {
        let Ok(container) = systasis::systasis_container! {
            register_container!(primary: &leaf::AppContainer<'env>);
            register_type_with!(Editor<'_, 'env> as IEditor, try || -> Result<Editor<'_, 'env>, Error> {
                let value = try_resolve_ref_mut_from!(IValue, primary)?;
                assert!(!value.0.is_empty());
                Ok(Editor(value))
            });
        }
        .build();
        let mut editor = container.try_resolve_i_editor().unwrap();
        // LOCAL_GUARD_AUTO_TRAIT_REJECTION
        assert!(matches!(
            primary.try_resolve_i_value_ref(),
            Err(Error::ValueAccessContention)
        ));
        editor.0.0 = replacement;
        drop(editor);
        assert_eq!(primary.try_resolve_i_value_ref().unwrap().0, replacement);
    }

    #[test]
    fn native_child_uses_its_local_guard_policy() {
        let initial = String::from("before");
        let replacement = String::from("after");
        leaf::run(&initial, |primary| check(primary, &replacement));
    }
}

mod transitive {
    use super::*;

    struct Wrapped<'a, 'env>(View<'a, 'env>);
    trait IWrapped {}
    impl IWrapped for Wrapped<'_, '_> {}

    #[systasis::container]
    fn check<'env>(primary: &leaf::AppContainer<'env>) {
        let Ok(container) = systasis::systasis_container! {
            register_container!(primary: &leaf::AppContainer<'env>);
            register_type_with!(View<'_, 'env> as IView, try || -> Result<View<'_, 'env>, Error> {
                Ok(View(try_resolve_ref_from!(IValue, primary)?))
            });
            register_type_with!(Wrapped<'_, 'env> as IWrapped, try || -> Result<Wrapped<'_, 'env>, Error> {
                let view = try_resolve!(IView)?;
                assert!(!view.0.0.is_empty());
                Ok(Wrapped(view))
            });
        }
        .build();
        let wrapped = container.try_resolve_i_wrapped().unwrap();
        assert_eq!(wrapped.0.0.0, "transitive");
        assert!(matches!(
            primary.try_resolve_i_value_ref_mut(),
            Err(Error::ValueAccessContention)
        ));
        drop(wrapped);
        assert!(primary.try_resolve_i_value_ref_mut().is_ok());
    }

    #[test]
    fn native_factory_forwards_child_context_to_reconstructed_factory() {
        let value = String::from("transitive");
        leaf::run(&value, check);
    }
}

mod nested_borrowed {
    use super::*;

    mod middle {
        use super::*;

        #[systasis::container]
        pub fn run<'a, 'env>(
            primary: &'a leaf::AppContainer<'env>,
            call: impl FnOnce(&AppContainer<'a, 'env>),
        ) {
            let Ok(container) = systasis::systasis_container! {
                register_container!(primary: &'a leaf::AppContainer<'env>);
            }
            .build();
            call(container);
        }
    }

    #[systasis::container]
    fn check<'a, 'env>(branch: &middle::AppContainer<'a, 'env>) {
        let Ok(container) = systasis::systasis_container! {
            register_container!(branch: &middle::AppContainer<'a, 'env>);
            register_type_with!(View<'_, 'env> as IView, try || -> Result<View<'_, 'env>, Error> {
                let value = try_resolve_ref_from!(IValue, branch::primary)?;
                assert!(!value.0.is_empty());
                Ok(View(value))
            });
        }
        .build();
        let view = container.try_resolve_i_view().unwrap();
        assert_eq!(view.0.0, "nested borrow");
        assert!(matches!(
            branch.primary().try_resolve_i_value_ref_mut(),
            Err(Error::ValueAccessContention)
        ));
        drop(view);
        assert!(branch.primary().try_resolve_i_value_ref_mut().is_ok());
    }

    #[test]
    fn nested_private_payload_retains_external_lifetime_without_added_bounds() {
        let value = String::from("nested borrow");
        leaf::run(&value, |primary| middle::run(primary, check));
    }
}
