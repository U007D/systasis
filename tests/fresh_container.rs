//! Fresh default constructors execute only when resolved.
#![forbid(unsafe_code)]

use std::{
    rc::Rc,
    sync::atomic::{AtomicUsize, Ordering},
};

static CONSTRUCTIONS: AtomicUsize = AtomicUsize::new(0);
struct Database(usize, Rc<()>);
impl Default for Database {
    fn default() -> Self {
        Self(CONSTRUCTIONS.fetch_add(1, Ordering::SeqCst), Rc::new(()))
    }
}
trait IDatabase {}
impl IDatabase for Database {}

#[systasis::container(require(Send, Sync))]
#[test]
fn fresh_values_are_not_constructed_or_retained_by_build() {
    let Ok(container) = systasis::systasis_container! {
        register_type!(Database as IDatabase);
    }
    .build();
    assert_eq!(CONSTRUCTIONS.load(Ordering::SeqCst), 0);
    let first = container.resolve_i_database();
    let second = container.resolve_i_database();
    assert_eq!((first.0, second.0), (0, 1));
    assert!(!Rc::ptr_eq(&first.1, &second.1));
}

mod dependency {
    use super::*;
    #[derive(Default)]
    struct OtherDatabase(Rc<()>);
    impl IDatabase for OtherDatabase {}
    struct Service(OtherDatabase);
    trait IService {}
    impl IService for Service {}
    #[systasis::container]
    #[test]
    fn stored_initializer_can_resolve_a_fresh_dependency() {
        let Ok(container) = systasis::systasis_container! {
            register_value!(Service(resolve!(IDatabase)): Service as IService);
            register_type!(OtherDatabase as IDatabase);
        }
        .build();
        let service = container.try_resolve_i_service().unwrap();
        assert_eq!(Rc::strong_count(&service.0.0), 1);
    }
}
