//! Pass only a named subcontainer's permitted registrations to a component.
#![deny(warnings)]
#![forbid(unsafe_code)]

use systasis::app_container::Error;

trait IDatabase {}
impl IDatabase for String {}

mod application {
    use super::Error;

    trait ILength {}
    impl ILength for usize {}

    // This function receives the database scope, not the application container.
    fn database_length(database: &primary::SubContainer<'_>) -> Result<usize, Error> {
        Ok(database.try_resolve_i_database_ref()?.len())
    }

    #[systasis::container]
    pub fn run(primary: &super::AppContainer) -> Result<(), Error> {
        let Ok(container) = systasis::systasis_container! {
            register_container!(primary: &super::AppContainer);
            register_type_with!(usize as ILength, try || -> Result<usize, Error> {
                // IDatabase identifies a registration in primary. No import
                // of the surrounding Rust trait is needed for this query.
                Ok(try_resolve_ref_from!(IDatabase, primary)?.len())
            });
        }
        .build();

        assert_eq!(database_length(container.primary())?, 11);
        assert_eq!(container.try_resolve_i_length()?, 11);
        container
            .primary()
            .try_resolve_i_database_ref_mut()?
            .push('!');
        assert_eq!(database_length(container.primary())?, 12);
        Ok(())
    }
}

#[systasis::container]
fn main() -> Result<(), Error> {
    let Ok(database) = systasis::systasis_container! {
        register_value!(String::from("application"): String as IDatabase);
    }
    .build();

    // The generated owner stays in main's scope throughout application::run.
    application::run(&database)
}
