//! Generic child payloads and constructor chains retain authored backing lifetimes.
#![forbid(unsafe_code)]
use systasis::app_container::Error;
struct Private(String);
trait IData {
    fn text(&self) -> &str;
}
impl IData for Private {
    fn text(&self) -> &str {
        &self.0
    }
}
struct Payload<'env, T>(&'env str, T);
trait IValue {}
impl<T> IValue for Payload<'_, T> {}
struct View<'a, 'env, T>(systasis::Ref<'a, Payload<'env, T>>);
trait IView {}
impl<T> IView for View<'_, '_, T> {}
mod leaf {
    use super::*;
    #[systasis::container]
    pub fn run<'env, T: IData>(
        label: &'env str,
        data: T,
        call: impl FnOnce(&AppContainer<'env, T>),
    ) {
        let Ok(container) = systasis::systasis_container! {
            register_value!(Payload(label,data): Payload<'env,T> as IValue);
        }
        .build();
        call(&container);
    }
}
macro_rules! ordered_case {
    ($module:ident, $inner:expr) => {
        mod $module {
            use super::*;
            trait IFirst {}
            impl<T> IFirst for View<'_, '_, T> {}
            #[systasis::container]
            fn run<'a, 'env, T:IData>(primary: &'a leaf::AppContainer<'env,T>) {
                let Ok(container) = systasis::systasis_container! {
                    register_container!(primary: &'a leaf::AppContainer<'env,T>);
                    register_type_with!(View<'a,'env,T> as IView, try || -> Result<View<'a,'env,T>,Error> {
                        let first=try_resolve!(IFirst)?;
                        assert_eq!(first.0.0,"data");
                        Ok(first)
                    });
                    register_type_with!(View<'a,'env,T> as IFirst, try || -> Result<View<'a,'env,T>,Error> { $inner });
                }.build();
                let view=container.try_resolve_i_view().unwrap();
                assert!(matches!(primary.try_resolve_i_value_ref_mut(),Err(Error::ValueAccessContention)));
                drop(view);
                assert!(primary.try_resolve_i_value_ref_mut().is_ok());
            }
            #[test]
            fn reordered_factories_retain_child_guard() {
                let label=String::from("data");
                leaf::run(&label,Private(label.clone()),run);
            }
        }
    };
}
ordered_case!(
    reconstructed_to_native,
    try_resolve_ref_from!(IValue, primary).map(View)
);
ordered_case!(native_to_native, {
    let value = try_resolve_ref_from!(IValue, primary)?;
    assert_eq!(value.0, "data");
    Ok(View(value))
});
mod explicit {
    use super::*;
    #[systasis::container]
    fn run<'a, 'env, T: IData>(primary: &'a leaf::AppContainer<'env, T>) {
        let Ok(container) = systasis::systasis_container! {
            register_container!(primary: &'a leaf::AppContainer<'env,T>);
            register_type_with!(View<'a,'env,T> as IView, try || -> Result<View<'a,'env,T>,Error> {
                let value = try_resolve_ref_from!(IValue,primary)?;
                assert_eq!(value.0,value.1.text());
                Ok(View(value))
            });
        }
        .build();
        let view: View<'a, 'env, T> = container.try_resolve_i_view().unwrap();
        assert_eq!(view.0.0, "data");
        assert!(matches!(
            primary.try_resolve_i_value_ref_mut(),
            Err(Error::ValueAccessContention)
        ));
        drop(view);
        assert!(primary.try_resolve_i_value_ref_mut().is_ok());
    }
    #[test]
    fn private_generic_payload_explicit_backing_output() {
        let label = String::from("data");
        leaf::run(&label, Private(label.clone()), run);
    }
}

mod authored_names {
    mod child {
        trait IValue {}
        impl IValue for u32 {}

        #[systasis::container]
        pub fn run(call: impl FnOnce(&AppContainer)) {
            let Ok(container) = systasis::systasis_container! {
                register_value!(7: u32 as IValue);
            }
            .build();
            call(&container);
        }
    }

    #[systasis::container]
    fn parent<
        '__item,
        __StoredIdentity,
        __ChildKey,
        __Rest,
        __Key,
        __Operation,
        __Restrictions,
        const __LOCAL: bool,
    >(
        primary: &'__item child::AppContainer,
        unrelated: (
            __StoredIdentity,
            __ChildKey,
            __Rest,
            __Key,
            __Operation,
            __Restrictions,
        ),
    ) {
        let Ok(container) = systasis::systasis_container! {
            register_container!(primary: &'__item child::AppContainer);
        }
        .build();
        assert_eq!(container.primary().resolve_i_value(), 7);
        drop(unrelated);
    }

    #[test]
    fn authored_generics_do_not_collide_with_stored_child_forwarding() {
        child::run(|primary| parent::<_, _, _, _, _, _, true>(primary, ((), (), (), (), (), ())));
    }
}
