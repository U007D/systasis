#![forbid(unsafe_code)]

pub mod public_result {
    pub struct Service(pub String);
    trait IService {}
    impl IService for Service {}

    #[systasis::container(require(Send, Sync))]
    pub fn run(config: String, inspect: fn(&AppContainer)) {
        let Ok(container) = systasis::systasis_container! {
            register_type_with!(Service as IService, move || Service(format!("{config}")));
        }.build();
        inspect(container);
    }
}

pub mod private_result {
    struct Service(String);
    trait IService {}
    impl IService for Service {}

    #[systasis::container]
    pub fn run(config: String, inspect: fn(&AppContainer)) {
        let Ok(container) = systasis::systasis_container! {
            register_type_with!(Service as IService, move || Service(format!("{config}")));
        }.build();
        let service: Service = container.resolve_i_service();
        assert_eq!(service.0, "private");
        inspect(container);
    }
}

pub mod generic {
    pub struct Service<'a, T>(pub &'a str, pub T);
    trait IService {}
    impl<T> IService for Service<'_, T> {}

    #[systasis::container]
    pub fn run<'a, T: Clone>(label: &'a str, value: T, inspect: fn(&AppContainer<'a, T>)) {
        let Ok(container) = systasis::systasis_container! {
            register_type_with!(Service<'a, T> as IService, move || {
                assert!(!label.is_empty());
                Service(label, value.clone())
            });
        }.build();
        inspect(container);
    }
}
