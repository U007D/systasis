//! Containers own their values and captures and may leave their construction scope.
#![forbid(unsafe_code)]

mod plain {
    #[systasis::container]
    pub fn init_container() -> Result<SystasisContainer, Box<dyn std::error::Error + Send + 'static>>
    {
        let container: SystasisContainer = systasis::systasis_container! {
            register_value!(42: u8 as Copy);
        }
        .build()?;
        Ok(container)
    }

    #[test]
    fn caller_owns_the_returned_container() {
        let container = init_container().unwrap();
        assert_eq!(container.resolve_copy(), 42);
    }
}

mod captured {
    struct View<'a>(&'a str);
    trait IView {}
    impl IView for View<'_> {}

    #[systasis::container(require(Send, Sync))]
    fn init_container(text: String) -> SystasisContainer {
        let Ok(container) = systasis::systasis_container! {
            register_type_with!(View<'_> as IView, move || View(text.as_str()));
        }
        .build();
        container
    }

    #[test]
    fn returned_container_owns_capture_and_lends_it_after_moving() {
        let container = init_container(String::from("owned"));
        let moved = container;
        assert_eq!(moved.resolve_i_view().0, "owned");
        std::thread::spawn(move || assert_eq!(moved.resolve_i_view().0, "owned"))
            .join()
            .unwrap();
    }
}

mod external_reference {
    #[systasis::container]
    fn init_container(value: &u8) -> SystasisContainer<'_> {
        let Ok(container) = systasis::systasis_container! {
            register_value!(value: &u8 as Copy);
        }
        .build();
        container
    }

    #[test]
    fn returned_container_may_retain_a_reference_supplied_by_its_caller() {
        let value = 42;
        let container = init_container(&value);
        assert!(core::ptr::eq(container.resolve_copy(), &value));
    }
}

mod borrowed_field {
    struct Service<'a> {
        database: &'a str,
    }
    trait IService {}
    impl IService for Service<'_> {}

    #[systasis::container]
    fn init_container(database: &str) -> SystasisContainer<'_> {
        let Ok(container) = systasis::systasis_container! {
            register_value!(Service { database }: Service<'_> as IService);
        }
        .build();
        container
    }

    #[test]
    fn owned_registration_can_contain_an_external_borrow() {
        let database = String::from("database");
        let service = {
            let container = init_container(&database);
            container.try_resolve_i_service().unwrap()
        };
        assert!(core::ptr::eq(service.database, database.as_str()));
    }
}

mod destruction {
    use std::{cell::Cell, rc::Rc};

    struct Tracked(Rc<Cell<usize>>);
    impl Drop for Tracked {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }
    trait ITracked {}
    impl ITracked for Tracked {}
    trait ICount {}
    impl ICount for usize {}

    #[systasis::container(require(!Sync))]
    fn init_container(value: Tracked, capture: Tracked) -> SystasisContainer {
        let Ok(container) = systasis::systasis_container! {
            register_value!(value: Tracked as ITracked);
            register_type_with!(usize as ICount, move || capture.0.get());
        }
        .build();
        container
    }

    #[test]
    fn dropping_returned_container_drops_its_value_and_capture_once() {
        let drops = Rc::new(Cell::new(0));
        let container = init_container(Tracked(drops.clone()), Tracked(drops.clone()));
        assert_eq!(container.resolve_i_count(), 0);
        drop(container);
        assert_eq!(drops.get(), 2);
    }
}

mod native_capture {
    trait IText {}
    impl IText for String {}

    #[systasis::container(require(Send, Sync))]
    fn init_container() -> SystasisContainer {
        let text = String::from("native owned capture");
        let Ok(container) = systasis::systasis_container! {
            register_type_with!(String as IText, move || text.clone());
        }
        .build();
        container
    }

    #[test]
    fn returned_native_constructor_retains_its_inferred_owned_capture() {
        let container = init_container();
        assert_eq!(container.resolve_i_text(), "native owned capture");
        assert_eq!(container.resolve_i_text(), "native owned capture");
    }
}
