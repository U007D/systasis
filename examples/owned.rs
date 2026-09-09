//! Ordinary generic constructors with container-selected owned dependencies.
#![deny(warnings)]
#![forbid(unsafe_code)]

use systasis::{app_container::Error, systasis_container};

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
trait IService {
    fn database_name(&self) -> &str;
}
impl<D: IDatabase> IService for Service<D> {
    fn database_name(&self) -> &str {
        self.database.name()
    }
}

#[systasis::container]
fn main() -> Result<(), Error> {
    let built = systasis_container! {
        register_value!(Database(String::from("application")): Database as IDatabase);
        register_value!(
            Service::new(try_resolve!(IDatabase)?):
            Service<registered_type!(IDatabase)> as IService
        );
    }
    .build::<Error>();
    let container = built?;
    assert!(matches!(
        container.try_resolve_i_database(),
        Err(Error::ValueAlreadyConsumed)
    ));
    let service = container.try_resolve_i_service()?;
    assert_eq!(service.database_name(), "application");
    Ok(())
}
