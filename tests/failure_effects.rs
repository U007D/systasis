//! Failure preserves prior consumption and ownership transferred into an error.
#![forbid(unsafe_code)]

use std::{cell::Cell, rc::Rc};
use systasis::container::Error;

#[derive(Debug)]
struct Resource {
    drops: Rc<Cell<usize>>,
}

impl Drop for Resource {
    fn drop(&mut self) {
        self.drops.set(self.drops.get() + 1);
    }
}

trait IResource {}
impl IResource for Resource {}
trait IOutput {}
impl IOutput for usize {}

#[derive(Debug)]
enum Failure {
    Access(Error),
    Rejected,
}

mod child {
    use super::*;

    #[systasis::container]
    pub fn with_resource(drops: Rc<Cell<usize>>, inspect: impl FnOnce(&SystasisContainer)) {
        let Ok(container) = systasis::systasis_container! {
            register_value!(Resource { drops }: Resource as IResource);
        }
        .build();
        inspect(&container);
    }
}

mod consumed_child {
    use super::*;

    #[systasis::container]
    fn fail(primary: &child::SystasisContainer, drops: &Cell<usize>) {
        let built = systasis::systasis_container! {
            register_container!(primary: &child::SystasisContainer);
            register_value!({
                // The dependency forces transfer before this initializer fails.
                let _resource = try_resolve!(IResource).map_err(Failure::Access)?;
                Err::<usize, Failure>(Failure::Rejected)?
            }: usize as IOutput);
            register_value!(
                try_resolve_from!(IResource, primary).map_err(Failure::Access)?:
                Resource as IResource
            );
        }
        .build::<Failure>();

        assert!(matches!(built, Err(Failure::Rejected)));
        assert_eq!(drops.get(), 1, "failed outer build drops transferred state");
        assert!(matches!(
            primary.try_resolve_i_resource(),
            Err(Error::ValueAlreadyConsumed)
        ));
    }

    #[test]
    fn failed_outer_build_does_not_restore_consumed_child_value() {
        let drops = Rc::new(Cell::new(0));
        child::with_resource(Rc::clone(&drops), |primary| fail(primary, &drops));
        assert_eq!(drops.get(), 1, "child destruction must not drop it twice");
    }
}

mod consumed_constructor_dependency {
    use super::*;

    #[systasis::container]
    #[test]
    fn failed_fresh_constructor_does_not_restore_consumed_dependency() {
        let drops: Rc<Cell<usize>> = Rc::new(Cell::new(0));
        let Ok(container) = systasis::systasis_container! {
            register_type_with!(usize as IOutput, try || -> Result<usize, Failure> {
                let resource = try_resolve!(IResource).map_err(Failure::Access)?;
                drop(resource);
                Err(Failure::Rejected)
            });
            register_value!(Resource { drops: Rc::clone(&drops) }: Resource as IResource);
        }
        .build();

        assert_eq!(drops.get(), 0, "building must not execute the constructor");
        assert!(matches!(
            container.try_resolve_i_output(),
            Err(Failure::Rejected)
        ));
        assert_eq!(drops.get(), 1);
        assert!(matches!(
            container.try_resolve_i_resource(),
            Err(Error::ValueAlreadyConsumed)
        ));
        assert!(matches!(
            container.try_resolve_i_output(),
            Err(Failure::Access(Error::ValueAlreadyConsumed))
        ));
        assert_eq!(
            drops.get(),
            1,
            "repeated failure must not duplicate ownership"
        );
    }
}

mod owned_error {
    use super::*;

    enum ResourceFailure {
        Access(Error),
        Rejected(Resource),
    }

    #[systasis::container]
    #[test]
    fn build_error_owns_transferred_resource_until_error_drop() {
        let drops: Rc<Cell<usize>> = Rc::new(Cell::new(0));
        let built = systasis::systasis_container! {
            register_value!({
                let resource = try_resolve!(IResource).map_err(ResourceFailure::Access)?;
                Err::<usize, ResourceFailure>(ResourceFailure::Rejected(resource))?
            }: usize as IOutput);
            register_value!(Resource { drops: Rc::clone(&drops) }: Resource as IResource);
        }
        .build::<ResourceFailure>();

        let error = match built {
            Err(error) => error,
            Ok(_) => panic!("the initializer always fails"),
        };
        match &error {
            ResourceFailure::Rejected(resource) => assert!(Rc::ptr_eq(&resource.drops, &drops)),
            ResourceFailure::Access(error) => panic!("resource must be available: {error}"),
        }
        assert_eq!(
            drops.get(),
            0,
            "build cleanup must not destroy error-owned data"
        );
        drop(error);
        assert_eq!(drops.get(), 1);
    }
}

mod child_owned_error {
    use super::*;

    enum ResourceFailure {
        Access(Error),
        Rejected(Resource),
    }

    #[systasis::container]
    fn fail(primary: &child::SystasisContainer) -> ResourceFailure {
        let built = systasis::systasis_container! {
            register_container!(primary: &child::SystasisContainer);
            register_value!({
                let resource = try_resolve_from!(IResource, primary).map_err(ResourceFailure::Access)?;
                Err::<usize, ResourceFailure>(ResourceFailure::Rejected(resource))?
            }: usize as IOutput);
        }.build::<ResourceFailure>();
        match built {
            Err(error) => error,
            Ok(_) => panic!("the initializer always fails"),
        }
    }

    #[test]
    fn build_error_owns_child_resource_until_error_drop() {
        let drops = Rc::new(Cell::new(0));
        child::with_resource(Rc::clone(&drops), |primary| {
            let error = fail(primary);
            match &error {
                ResourceFailure::Rejected(resource) => assert!(Rc::ptr_eq(&resource.drops, &drops)),
                ResourceFailure::Access(error) => panic!("resource must be available: {error}"),
            }
            assert!(matches!(
                primary.try_resolve_i_resource(),
                Err(Error::ValueAlreadyConsumed)
            ));
            assert_eq!(drops.get(), 0);
            drop(error);
            assert_eq!(drops.get(), 1);
        });
        assert_eq!(drops.get(), 1);
    }
}
