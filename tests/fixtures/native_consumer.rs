#![forbid(unsafe_code)]

fn public(container: &native_provider::public_result::AppContainer) {
    let service: native_provider::public_result::Service = container.resolve_i_service();
    assert_eq!(service.0, "public");
}
fn private(_: &native_provider::private_result::AppContainer) {}
fn generic<'a>(container: &native_provider::generic::AppContainer<'a, String>) {
    let service: native_provider::generic::Service<'a, String> = container.resolve_i_service();
    assert_eq!(service.0, "label");
    assert_eq!(service.1, "owned");
}

mod parent {
    use native_provider::public_result::AppContainer as Child;

    fn receive(scope: &primary::SubContainer<'_>) -> String {
        scope.resolve_i_service().0
    }

    #[systasis::container]
    pub fn inspect(child: &Child) {
        let primary = child;
        let replica = child;
        let Ok(container) = systasis::systasis_container! {
            register_container!(primary: &Child);
            register_container!(replica: &Child);
        }.build();
        assert_eq!(receive(container.primary()), "public");
        assert_eq!(container.replica().resolve_i_service().0, "public");
    }
}

mod borrowing_parent {
    use native_provider::borrowed_child::{AppContainer as Child, Value};
    use systasis::app_container::Error;

    struct View<'a, 'env>(systasis::Ref<'a, Value<'env>>);
    trait IView {}
    impl IView for View<'_, '_> {}

    fn receive<'call, 'env>(container: &'call AppContainer<'_, '_, 'env>) -> View<'call, 'env> {
        container.try_resolve_i_view().unwrap()
    }

    fn scoped_receive<'a, 'env>(scope: &primary::SubContainer<'a, 'env>) -> systasis::Ref<'a, Value<'env>> {
        scope.try_resolve_i_value_ref().unwrap()
    }

    #[systasis::container(require(Send, Sync))]
    pub fn inspect<'env>(primary: &Child<'env>) {
        let Ok(container) = systasis::systasis_container! {
            register_container!(primary: &Child<'env>);
            register_type_with!(View<'_, 'env> as IView, try || -> Result<View<'_, 'env>, Error> {
                let value = try_resolve_ref_from!(IValue, primary)?;
                assert!(!value.0.is_empty());
                Ok(View(value))
            });
        }.build();
        let view = receive(container);
        assert_eq!(view.0.0, "borrowed child");
        assert_eq!(scoped_receive(container.primary()).0, "borrowed child");
        assert!(matches!(primary.try_resolve_i_value(), Err(Error::ValueAccessContention)));
        drop(view);
        assert_eq!(primary.try_resolve_i_value().unwrap().0, "borrowed child");
    }
}

fn main() {
    native_provider::public_result::run(String::from("public"), public);
    native_provider::public_result::run(String::from("public"), parent::inspect);
    native_provider::private_result::run(String::from("private"), private);
    let label = String::from("label");
    native_provider::generic::run(&label, String::from("owned"), generic);
    let child_value: String = String::from("borrowed child");
    native_provider::borrowed_child::run(&child_value, borrowing_parent::inspect);
}
