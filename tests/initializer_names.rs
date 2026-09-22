//! Supported initializer scopes keep caller names outside the reserved prefix.
//! Reserved-name collision probes remain research evidence, not supported usage.
#![forbid(unsafe_code)]

use systasis::app_container::Error;

mod imported {
    // Ordinary names resembling implementation roles remain available to callers.
    #[allow(non_upper_case_globals)]
    pub const slot_0: &str = "outside";
    pub type IValue = bool;
}

mod local {
    use super::{Error, imported};

    trait IValue {}
    impl IValue for String {}
    trait IUnused {}
    impl IUnused for String {}
    trait IOutput {}
    impl IOutput for String {}

    #[systasis::container]
    #[test]
    fn glob_names_keep_authored_meanings_and_queries_use_registered_values() -> Result<(), Error> {
        // Deliberately unannotated: initializer inputs keep ordinary inference.
        let mut events = Vec::new();
        let container = systasis::systasis_container! {
            register_value!(String::from("actual"): String as IValue);
            register_value!(String::from("unused"): String as IUnused);
            register_value!({
                use imported::*;
                let should_take: IValue = false;
                events.push(slot_0);
                if should_take {
                    drop(try_resolve!(IUnused)?);
                }
                try_resolve!(IValue)?
            }: String as IOutput);
        }
        .build::<Error>()?;
        events.push("after");
        assert_eq!(events, ["outside", "after"]);
        assert_eq!(container.try_resolve_i_output()?, "actual");
        assert!(matches!(
            container.try_resolve_i_value(),
            Err(Error::ValueAlreadyConsumed)
        ));
        assert_eq!(container.try_resolve_i_unused()?, "unused");
        Ok(())
    }
}

mod branching {
    use super::{Error, imported};

    trait IValue {}
    impl IValue for String {}
    trait IOutput {}
    impl IOutput for String {}

    #[systasis::container]
    fn run(take_branch: bool) -> Result<(), Error> {
        let container = systasis::systasis_container! {
            register_value!(String::from("actual"): String as IValue);
            register_value!({
                let input = String::from("retained");
                'output: {
                    {
                        use imported::*;
                        let take_branch: IValue = take_branch;
                        assert_eq!(slot_0, "outside");
                        if take_branch {
                            drop(input);
                            break 'output try_resolve!(IValue)?;
                        }
                    }
                    // Valid only if the move stays in the diverging branch.
                    assert_eq!(input, "retained");
                    try_resolve!(IValue)?
                }
            }: String as IOutput);
        }
        .build::<Error>()?;
        assert_eq!(container.try_resolve_i_output()?, "actual");
        assert!(matches!(
            container.try_resolve_i_value(),
            Err(Error::ValueAlreadyConsumed)
        ));
        Ok(())
    }

    #[test]
    fn nested_glob_keeps_conditional_moves_and_labelled_breaks() {
        for take_branch in [false, true] {
            run(take_branch).unwrap();
        }
    }
}

mod leaf {
    trait IValue {}
    impl IValue for String {}

    #[systasis::container]
    pub fn run(call: impl FnOnce(&SystasisContainer)) {
        let Ok(container) = systasis::systasis_container! {
            register_value!(String::from("child"): String as IValue);
        }
        .build();
        call(&container);
    }
}

mod middle {
    use super::leaf;

    #[systasis::container]
    pub fn run<'a>(
        primary: &'a leaf::SystasisContainer,
        call: impl FnOnce(&SystasisContainer<'a>),
    ) {
        let Ok(container) = systasis::systasis_container! {
            register_container!(primary: &'a leaf::SystasisContainer);
        }
        .build();
        call(&container);
    }
}

mod outer {
    use super::{Error, imported, middle};

    trait IOutput {}
    impl IOutput for String {}

    #[systasis::container]
    pub fn run<'a>(branch: &middle::SystasisContainer<'a>) -> Result<(), Error> {
        let container = systasis::systasis_container! {
            register_container!(branch: &middle::SystasisContainer<'a>);
            register_value!({
                use imported::*;
                let unrelated: IValue = false;
                assert!(!unrelated);
                assert_eq!(slot_0, "outside");
                try_resolve_from!(IValue, branch::primary)?
            }: String as IOutput);
        }
        .build::<Error>()?;
        assert_eq!(container.try_resolve_i_output()?, "child");
        Ok(())
    }
}

#[test]
fn nested_child_lookup_ignores_glob_imported_type_alias() {
    leaf::run(|primary| {
        middle::run(primary, |branch| outer::run(branch).unwrap());
        assert!(matches!(
            primary.try_resolve_i_value(),
            Err(Error::ValueAlreadyConsumed)
        ));
    });
}
