//! Namespace suffixes in child paths preserve registration identity and types.
#![forbid(unsafe_code)]
use systasis::container::Error;

mod child {
    pub trait IValue {
        fn length(&self) -> usize;
    }
    impl IValue for String {
        fn length(&self) -> usize {
            self.len()
        }
    }
    #[systasis::container]
    pub fn run(call: impl FnOnce(&SystasisContainer)) {
        let Ok(container) = systasis::systasis_container! {
            register_value!(String::from("default"): String as dyn IValue);
            register_value!(String::from("metrics"): String as dyn IValue in metrics);
        }
        .build();
        call(&container);
    }
}

mod outer {
    use super::*;
    trait ILength {}
    impl ILength for usize {}
    trait ICopied {}
    impl ICopied for String {}
    trait IDefault {}
    impl IDefault for usize {}
    trait ILater {}
    impl ILater for usize {}
    #[systasis::container]
    pub fn run(primary: &child::SystasisContainer) -> Result<(), Error> {
        let container = systasis::systasis_container! {
            register_container!(primary: &child::SystasisContainer);
            register_value!(try_resolve_clone_from!(IValue, primary::metrics)?: resolve_type_from!(IValue, primary::metrics) as ICopied);
            register_value!({
                let guard = try_resolve_dyn_ref_from!(IValue, primary::metrics)?;
                let value: &resolve_type_from!(dyn IValue, primary::metrics) = &*guard;
                value.length()
            }: usize as ILength);
            register_value!(try_resolve_ref_from!(IValue, primary::default)?.len(): usize as IDefault);
            register_type_with!(usize as ILater, try || -> Result<usize, Error> {
                Ok(try_resolve_ref_from!(IValue, primary::metrics)?.len())
            });
        }.build::<Error>()?;
        assert_eq!(container.resolve_i_length(), 7);
        assert_eq!(container.resolve_i_default(), 7);
        assert_eq!(container.try_resolve_i_copied()?, "metrics");
        assert_eq!(container.try_resolve_i_later()?, 7);
        container
            .primary()
            .try_resolve_i_value_ref_mut_in_metrics()?
            .push('!');
        assert_eq!(container.try_resolve_i_later()?, 8);
        Ok(())
    }
}

#[test]
fn child_paths_select_named_and_explicit_default_namespaces() {
    child::run(|child| outer::run(child).unwrap());
}
