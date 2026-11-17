#![forbid(unsafe_code)]

fn public<F: native_provider::public_result::SystasisFactories>(container: &native_provider::public_result::SystasisContainer<F>) {
    let service: native_provider::public_result::Service = container.resolve_i_service();
    assert_eq!(service.0, "public");
}
fn private<F: native_provider::private_result::SystasisFactories>(_: &native_provider::private_result::SystasisContainer<F>) {}
fn generic<'a, F: native_provider::generic::SystasisFactories<'a, String>>(container: &native_provider::generic::SystasisContainer<'a, String, F>) {
    let service: native_provider::generic::Service<'a, String> = container.resolve_i_service();
    assert_eq!(service.0, "label");
    assert_eq!(service.1, "owned");
}

mod parent {
    use native_provider::public_result::{SystasisContainer as Child, SystasisFactories};

    fn receive<F: SystasisFactories>(scope: &primary::SubContainer<'_, F>) -> String {
        scope.resolve_i_service().0
    }

    #[systasis::container]
    pub fn inspect<F: SystasisFactories>(child: &Child<F>) {
        let primary = child;
        let replica = child;
        let Ok(container) = systasis::systasis_container! {
            register_container!(primary: &Child<F>);
            register_container!(replica: &Child<F>);
        }
        .build();
        assert_eq!(receive(container.primary()), "public");
        assert_eq!(container.replica().resolve_i_service().0, "public");
    }
}

mod borrowing_parent {
    use native_provider::borrowed_child::{SystasisContainer as Child, Value};
    use systasis::container::Error;

    struct View<'env>(Value<'env>);
    trait IView {}
    impl IView for View<'_> {}

    fn receive<'env, F: SystasisFactories<'env>>(container: &SystasisContainer<'_, 'env, F>) -> View<'env> {
        container.try_resolve_i_view().unwrap()
    }

    fn scoped_receive<'a, 'env>(
        scope: &primary::SubContainer<'a, 'env>,
    ) -> Result<Value<'env>, Error> {
        scope.try_resolve_i_value()
    }

    #[systasis::container(require(Send, Sync))]
    pub fn inspect<'env>(primary: &Child<'env>) {
        let Ok(container) = systasis::systasis_container! {
            register_container!(primary: &Child<'env>);
            register_type_with!(View<'env> as IView, try || -> Result<View<'env>, Error> {
                let value = try_resolve_from!(IValue, primary)?;
                assert!(!value.0.is_empty());
                Ok(View(value))
            });
        }
        .build();
        let view = receive(&container);
        assert_eq!(view.0.0, "borrowed child");
        assert!(matches!(
            scoped_receive(container.primary()),
            Err(Error::ValueAlreadyConsumed)
        ));
        assert!(matches!(
            primary.try_resolve_i_value(),
            Err(Error::ValueAlreadyConsumed)
        ));
        drop(container);
        assert_eq!(view.0.0, "borrowed child");
    }
}

mod nested_borrowing_parent {
    use native_provider::{borrowed_branch::SystasisContainer as Branch, borrowed_child::Value};
    use systasis::container::Error;

    struct View<'env>(Value<'env>);
    trait IView {}
    impl IView for View<'_> {}

    fn receive<'a, 'env, F: SystasisFactories<'a, 'env>>(container: &SystasisContainer<'_, 'a, 'env, F>) -> View<'env> {
        container.try_resolve_i_view().unwrap()
    }

    fn scoped_receive<'b, 'a, 'env>(
        scope: &branch::SubContainer<'b, 'a, 'env>,
    ) -> Result<Value<'env>, Error> {
        scope.primary().try_resolve_i_value()
    }

    #[systasis::container(require(Send, Sync))]
    pub fn inspect<'a, 'env>(branch: &Branch<'a, 'env>) {
        let Ok(container) = systasis::systasis_container! {
            register_container!(branch: &Branch<'a, 'env>);
            register_type_with!(View<'env> as IView, try || -> Result<View<'env>, Error> {
                let value = try_resolve_from!(IValue, branch::primary)?;
                assert!(!value.0.is_empty());
                Ok(View(value))
            });
        }
        .build();
        let view = receive(&container);
        assert_eq!(view.0.0, "nested borrowed child");
        assert!(matches!(
            scoped_receive(container.branch()),
            Err(Error::ValueAlreadyConsumed)
        ));
        assert!(matches!(
            branch.primary().try_resolve_i_value(),
            Err(Error::ValueAlreadyConsumed)
        ));
        drop(container);
        assert_eq!(view.0.0, "nested borrowed child");
    }
}

fn main() {
    let container = native_provider::public_result::build(String::from("public"));
    public(&container);
    parent::inspect(&container);
    private(&native_provider::private_result::build(String::from("private")));
    let label = String::from("label");
    generic(&native_provider::generic::build(&label, String::from("owned")));
    let child_value: String = String::from("borrowed child");
    native_provider::borrowed_child::run(&child_value, borrowing_parent::inspect);
    let nested_child_value: String = String::from("nested borrowed child");
    native_provider::borrowed_child::run(&nested_child_value, |primary| {
        native_provider::borrowed_branch::run(primary, nested_borrowing_parent::inspect);
    });
}
