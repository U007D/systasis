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

    fn receive<'call, 'env>(container: &'call AppContainer<'_, 'env>) -> View<'call, 'env> {
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
        let view = receive(&container);
        assert_eq!(view.0.0, "borrowed child");
        assert_eq!(scoped_receive(container.primary()).0, "borrowed child");
        assert!(matches!(primary.try_resolve_i_value(), Err(Error::ValueAccessContention)));
        drop(view);
        assert_eq!(primary.try_resolve_i_value().unwrap().0, "borrowed child");
    }
}

mod nested_borrowing_parent {
    use native_provider::{borrowed_branch::AppContainer as Branch, borrowed_child::Value};
    use systasis::app_container::Error;

    struct View<'a, 'env>(systasis::Ref<'a, Value<'env>>);
    trait IView {}
    impl IView for View<'_, '_> {}

    fn receive<'call, 'a, 'env>(container: &'call AppContainer<'_, 'a, 'env>) -> View<'call, 'env> {
        container.try_resolve_i_view().unwrap()
    }

    fn scoped_receive<'b, 'a, 'env>(scope: &branch::SubContainer<'b, 'a, 'env>) -> systasis::Ref<'b, Value<'env>> {
        scope.primary().try_resolve_i_value_ref().unwrap()
    }

    #[systasis::container(require(Send, Sync))]
    pub fn inspect<'a, 'env>(branch: &Branch<'a, 'env>) {
        let Ok(container) = systasis::systasis_container! {
            register_container!(branch: &Branch<'a, 'env>);
            register_type_with!(View<'_, 'env> as IView, try || -> Result<View<'_, 'env>, Error> {
                let value = try_resolve_ref_from!(IValue, branch::primary)?;
                assert!(!value.0.is_empty());
                Ok(View(value))
            });
        }.build();
        let view = receive(&container);
        assert_eq!(view.0.0, "nested borrowed child");
        assert_eq!(scoped_receive(container.branch()).0, "nested borrowed child");
        assert!(matches!(branch.primary().try_resolve_i_value(), Err(Error::ValueAccessContention)));
        drop(view);
        assert_eq!(branch.primary().try_resolve_i_value().unwrap().0, "nested borrowed child");
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
    let nested_child_value: String = String::from("nested borrowed child");
    native_provider::borrowed_child::run(&nested_child_value, |primary| {
        native_provider::borrowed_branch::run(primary, nested_borrowing_parent::inspect);
    });
}
