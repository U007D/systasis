//! Unchecked child forwarding retains acquisition guards across nested scopes.
#![cfg(feature = "resolve_unchecked")]
#![cfg_attr(miri, feature(never_type))]

use systasis::app_container::Error;

mod leaf {
    pub trait IValue {}
    impl IValue for String {}
    #[systasis::container]
    pub fn run(call: impl FnOnce(&AppContainer)) {
        let Ok(container) = systasis::systasis_container! {
            register_value!(String::from("value"): String as IValue);
            register_value!(String::from("metric"): String as IValue in metrics);
        }
        .build();
        call(container);
    }
}

mod branch {
    use super::*;
    #[systasis::container]
    pub fn run<'a>(primary: &'a leaf::AppContainer, call: impl FnOnce(&AppContainer<'a>)) {
        let Ok(container) = systasis::systasis_container! {
            register_container!(primary: &'a leaf::AppContainer);
        }
        .build();
        call(container);
    }
}

mod outer {
    use super::*;
    trait ILength {}
    impl ILength for usize {}
    #[systasis::container]
    pub fn run<'a>(branch: &crate::branch::AppContainer<'a>) -> Result<(), Error> {
        let container = systasis::systasis_container! {
            register_container!(branch: &crate::branch::AppContainer<'a>);
            register_value!({
                // SAFETY: freshly built child is present with no mutable guard.
                let read = unsafe { resolve_ref_unchecked_from!(IValue, branch::primary) };
                let contended = try_resolve_ref_mut_from!(IValue, branch::primary);
                assert!(matches!(contended, Err(Error::ValueAccessContention)));
                drop(contended);
                assert_eq!(&*read, "value");
                drop(read);
                // SAFETY: shared guard was dropped; no other borrower exists.
                let mut write = unsafe { resolve_ref_mut_unchecked_from!(IValue, branch::primary) };
                write.push('!');
                let contended = try_resolve_ref_from!(IValue, branch::primary);
                assert!(matches!(contended, Err(Error::ValueAccessContention)));
                drop(contended);
                drop(write);
                // SAFETY: both guards were dropped; value remains present.
                let owned = unsafe { resolve_unchecked_from!(IValue, branch::primary) };
                assert_eq!(owned, "value!");
                // SAFETY: this independent namespace value has not been accessed.
                let metric = unsafe { resolve_unchecked_from!(IValue, branch::primary::metrics) };
                assert_eq!(metric, "metric");
                owned.len()
            }: usize as ILength);
        }
        .build::<Error>()?;
        assert_eq!(container.resolve_i_length(), 6);
        assert!(matches!(
            container.branch().primary().try_resolve_i_value(),
            Err(Error::ValueAlreadyConsumed)
        ));
        Ok(())
    }
}

#[test]
fn nested_unchecked_queries_hold_guards_and_release_before_consumption() {
    leaf::run(|primary| {
        branch::run(primary, |branch| outer::run(branch).unwrap());
    });
}
