//! Child scope types follow Rust aliases and retain independent instance state.
#![forbid(unsafe_code)]

mod generic_child {
    pub trait IValue {}
    impl<T> IValue for T {}
    #[systasis::container]
    pub fn run<T: Copy>(value: T, use_child: impl FnOnce(&SystasisContainer<T>)) {
        let Ok(container) = systasis::systasis_container! {
            register_value!(value: T as IValue);
        }
        .build();
        use_child(&container);
    }
}

mod generic_outer {
    use super::generic_child::SystasisContainer as Renamed;
    type Alias<T> = Renamed<T>;

    fn receive<T: Copy>(scope: &primary::SubContainer<'_, T>) -> T {
        scope.resolve_i_value()
    }

    #[systasis::container]
    pub fn run<T: Copy + PartialEq>(primary: &Alias<T>, replica: &Alias<T>, first: T, second: T) {
        let Ok(container) = systasis::systasis_container! {
            register_container!(primary: &Alias<T>);
            register_container!(replica: &Alias<T>);
        }
        .build();
        assert!(receive(container.primary()) == first);
        assert!(container.replica().resolve_i_value() == second);
        assert!(core::ptr::eq(container.primary(), container.primary()));
    }
}

#[test]
fn generic_alias_and_import_rename_preserve_distinct_child_instances() {
    generic_child::run(11_u32, |first| {
        generic_child::run(29_u32, |second| generic_outer::run(first, second, 11, 29));
    });
}

mod borrowed_child {
    pub trait IText {}
    impl IText for String {}
    #[systasis::container]
    pub fn run<'a>(config: &'a str, use_child: impl FnOnce(&SystasisContainer<'a>)) {
        let Ok(container) = systasis::systasis_container! {
            register_type_with!(String as IText, move || config.to_owned());
        }
        .build();
        use_child(&container);
    }
}

mod borrowed_outer {
    use super::borrowed_child::SystasisContainer as Renamed;
    type Alias<'a> = Renamed<'a>;

    fn receive(scope: &primary::SubContainer<'_, '_>) -> String {
        scope.resolve_i_text()
    }

    #[systasis::container]
    pub fn run<'a>(primary: &Alias<'a>, replica: &Alias<'a>, first: &str, second: &str) {
        let Ok(container) = systasis::systasis_container! {
            register_container!(primary: &Alias<'a>);
            register_container!(replica: &Alias<'a>);
        }
        .build();
        assert_eq!(receive(container.primary()), first);
        assert_eq!(container.replica().resolve_i_text(), second);
    }
}

#[test]
fn nonstatic_constructor_captures_survive_aliases_and_shared_child_scopes() {
    let first = String::from("primary");
    let second = String::from("replica");
    borrowed_child::run(&first, |primary| {
        borrowed_child::run(&second, |replica| {
            borrowed_outer::run(primary, replica, &first, &second)
        });
    });
    assert_eq!(first, "primary");
    assert_eq!(second, "replica");
}
