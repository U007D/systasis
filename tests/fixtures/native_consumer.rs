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

fn main() {
    native_provider::public_result::run(String::from("public"), public);
    native_provider::public_result::run(String::from("public"), parent::inspect);
    native_provider::private_result::run(String::from("private"), private);
    let label = String::from("label");
    native_provider::generic::run(&label, String::from("owned"), generic);
}
