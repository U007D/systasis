//! Named child access uses references to inline descriptors, not copied views.
#![forbid(unsafe_code)]

mod child {
    pub trait IValue {}
    impl IValue for u32 {}
    #[systasis::container]
    pub fn run(use_child: impl FnOnce(&AppContainer)) {
        let Ok(container) = systasis::systasis_container! {
            register_value!(7: u32 as IValue);
        }
        .build();
        use_child(&container);
    }
}

mod outer {
    trait ILength {}
    impl ILength for u32 {}
    type Alias = super::child::AppContainer;
    fn scoped_lookup(scope: &primary::SubContainer<'_>) -> u32 {
        scope.resolve_i_value()
    }
    #[systasis::container]
    pub fn run(primary: &Alias, replica: &Alias) {
        let Ok(container) = systasis::systasis_container! {
            register_container!(primary: &Alias);
            register_container!(replica: &Alias);
            register_value!(resolve_from!(IValue, primary): resolve_type_from!(IValue, replica) as ILength);
        }.build();
        assert_eq!(scoped_lookup(container.primary()), 7);
        assert_eq!(container.replica().resolve_i_value(), 7);
        assert_eq!(container.resolve_i_length(), 7);
        assert!(core::ptr::eq(container.primary(), container.primary()));
    }
}

#[test]
fn aliases_and_two_instances_have_nameable_shared_child_scopes() {
    child::run(|child| outer::run(child, child));
}
