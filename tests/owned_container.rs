//! Owned dependencies retain ordinary user type and constructor semantics.
#![forbid(unsafe_code)]

use systasis::app_container::Error;

trait IDatabase {
    fn name(&self) -> &str;
}
struct Database(String);
impl IDatabase for Database {
    fn name(&self) -> &str {
        &self.0
    }
}
struct Service<D: IDatabase> {
    database: D,
}
impl<D: IDatabase> Service<D> {
    fn new(database: D) -> Self {
        Self { database }
    }
}
trait IService {}
impl<D: IDatabase> IService for Service<D> {}

macro_rules! scenario {
    ($module:ident, ($($requirements:tt)*)) => {
        mod $module {
            use super::*;
            #[systasis::container($($requirements)*)]
            #[test]
            fn moves_dependency_into_ordinary_service() -> Result<(), Error> {
                let built = systasis_container! {
                    register_value!(Service::new(try_resolve!(IDatabase)?): Service<registered_type!(IDatabase)> as IService);
                    register_value!(Database(String::from("db")): Database as IDatabase);
                }.build::<Error>();
                let container = built?;
                accepts_named_container(container);
                assert!(matches!(container.try_resolve_i_database(), Err(Error::ValueAlreadyConsumed)));
                let service = container.try_resolve_i_service()?;
                assert_eq!(service.database.name(), "db");
                assert!(matches!(container.try_resolve_i_service(), Err(Error::ValueAlreadyConsumed)));
                Ok(())
            }
            fn accepts_named_container(_: &AppContainer) {}
        }
    };
}
scenario!(synchronized, (require(Send, Sync)));
scenario!(local, (require(Send, !Sync)));

mod overrides {
    use super::*;
    #[systasis::container]
    #[test]
    fn discarded_initializer_and_dependencies_are_not_evaluated() -> Result<(), Error> {
        let built = systasis_container! {
            register_value!(try_resolve!(IMissing)?: MissingType as IDatabase);
            register_value!(Service::new(try_resolve!(IDatabase)?): Service<registered_type!(IDatabase)> as IService);
            register_value!(Database(String::from("winner")): Database as IDatabase);
        }.build::<Error>();
        assert_eq!(built?.try_resolve_i_service()?.database.name(), "winner");
        Ok(())
    }
}

mod plain {
    trait IValue {}
    impl IValue for u32 {}
    #[systasis::container]
    #[test]
    fn copy_values_are_repeatedly_available() {
        let built = systasis_container! {
            register_value!(7_u32: u32 as IValue);
        }
        .build();
        let Ok(container) = built;
        assert_eq!(container.resolve_i_value(), 7);
        assert_eq!(*container.resolve_i_value_ref(), 7);
        assert_eq!(container.resolve_i_value_clone(), 7);
    }
}

mod failure {
    use super::Error;
    use std::{cell::Cell, rc::Rc};
    struct Tracked(Rc<Cell<usize>>);
    impl Drop for Tracked {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }
    trait IValue {}
    impl IValue for Tracked {}
    trait IFirst {}
    impl IFirst for Tracked {}
    trait ISecond {}
    impl ISecond for Tracked {}
    #[systasis::container(require(!Sync))]
    #[test]
    fn failed_build_drops_transferred_value_once() {
        let drops = Rc::new(Cell::new(0));
        let built = systasis_container! {
            register_value!(Tracked(drops.clone()): Tracked as IValue);
            register_value!(try_resolve!(IValue)?: registered_type!(IValue) as IFirst);
            register_value!(try_resolve!(IValue)?: registered_type!(IValue) as ISecond);
        }
        .build::<Error>();
        assert!(matches!(built, Err(Error::ValueAlreadyConsumed)));
        assert_eq!(drops.get(), 1);
    }
}
