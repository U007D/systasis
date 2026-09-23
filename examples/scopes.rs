//! Pass a named subcontainer to a component instead of the whole container.
#![deny(warnings)]
#![forbid(unsafe_code)]

use systasis::container::Error;

trait IDatabase {}
impl IDatabase for String {}

mod application {
    use super::Error;

    trait ILength {}
    impl ILength for usize {}

    // This function receives the database scope, not the application container.
    fn database_length(database: &primary::SubContainer<'_>) -> usize {
        database.resolve_i_database_clone().len()
    }

    #[systasis::container]
    pub fn run(primary: &super::SystasisContainer) -> Result<(), Error> {
        let Ok(container) = systasis::systasis_container! {
            register_container!(primary: &super::SystasisContainer);
            register_type_with!(usize as ILength, try || -> Result<usize, Error> {
                // IDatabase identifies a registration in primary. No import
                // of the surrounding Rust trait is needed for this query.
                Ok(resolve_clone_from!(IDatabase, primary).len())
            });
        }
        .build();

        assert_eq!(database_length(container.primary()), 11);
        assert_eq!(container.try_resolve_i_length()?, 11);
        let mut cloned = container.primary().resolve_i_database_clone();
        cloned.push('!');
        assert_eq!(cloned, "application!");
        assert_eq!(database_length(container.primary()), 11);
        Ok(())
    }
}

#[systasis::container]
fn main() -> Result<(), Error> {
    let Ok(database) = systasis::systasis_container! {
        register_value!(String::from("application"): String as IDatabase);
    }
    .build();

    // main owns the container; application::run borrows it.
    application::run(&database)
}
