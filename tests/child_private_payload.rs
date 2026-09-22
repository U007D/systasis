//! Child metadata must not expose a private payload through normalized projections.
#![forbid(unsafe_code)]

struct Borrowed<'a>(&'a str);
trait IValue {}
impl IValue for Borrowed<'_> {}
struct View<'a, 'env>(systasis::Ref<'a, Borrowed<'env>>);
trait IView {}
impl IView for View<'_, '_> {}

mod child {
    use super::*;

    #[systasis::container]
    pub fn run<'env>(value: &'env str, call: impl FnOnce(&AppContainer<'env>)) {
        let Ok(container) = systasis::systasis_container! {
            register_value!(Borrowed(value): Borrowed<'env> as IValue);
        }
        .build();
        call(&container);
    }
}

mod parent {
    use super::*;

    #[systasis::container]
    pub fn check<'a, 'env>(primary: &'a child::AppContainer<'env>) {
        let Ok(container) = systasis::systasis_container! {
            register_container!(primary: &'a child::AppContainer<'env>);
            register_type_with!(View<'_, 'env> as IView, try || -> Result<View<'_, 'env>, systasis::app_container::Error> {
                Ok(View(try_resolve_ref_from!(IValue, primary)?))
            });
        }
        .build();
        let view = container.try_resolve_i_view().unwrap();
        assert_eq!(view.0.0, "borrowed");
        assert!(matches!(
            primary.try_resolve_i_value_ref_mut(),
            Err(systasis::app_container::Error::ValueAccessContention)
        ));
        drop(view);
        assert!(primary.try_resolve_i_value_ref_mut().is_ok());
    }
}

#[test]
fn private_child_payload_and_parent_result_remain_private() {
    let value = String::from("borrowed");
    child::run(value.as_str(), parent::check);
}
