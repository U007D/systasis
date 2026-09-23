//! Child metadata must not expose a private payload through normalized projections.
#![forbid(unsafe_code)]

struct Borrowed<'a>(&'a str);
trait IValue {}
impl IValue for Borrowed<'_> {}
struct View<'env>(Borrowed<'env>);
trait IView {}
impl IView for View<'_> {}

mod child {
    use super::*;

    #[systasis::container]
    pub fn run<'env>(value: &'env str, call: impl FnOnce(&SystasisContainer<'env>)) {
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
    pub fn check<'a, 'env>(primary: &'a child::SystasisContainer<'env>) {
        let Ok(container) = systasis::systasis_container! {
            register_container!(primary: &'a child::SystasisContainer<'env>);
            register_type_with!(View<'env> as IView, try || -> Result<View<'env>, systasis::container::Error> {
                Ok(View(try_resolve_from!(IValue, primary)?))
            });
        }
        .build();
        let view = container.try_resolve_i_view().unwrap();
        assert_eq!(view.0.0, "borrowed");
        assert!(matches!(
            primary.try_resolve_i_value(),
            Err(systasis::container::Error::ValueAlreadyConsumed)
        ));
        assert!(matches!(
            container.try_resolve_i_view(),
            Err(systasis::container::Error::ValueAlreadyConsumed)
        ));
    }
}

#[test]
fn private_child_payload_and_parent_result_remain_private() {
    let value = String::from("borrowed");
    child::run(value.as_str(), parent::check);
}
