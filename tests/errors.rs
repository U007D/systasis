//! Public error behavior in both runtime configurations.
use std::error::Error as _;
use systasis::app_container::Error;

#[test]
fn resolution_errors_support_the_required_value_traits() {
    fn assert_value_traits<T: Clone + Copy + core::fmt::Debug + Eq + PartialEq>() {}
    assert_value_traits::<Error>();

    for error in [Error::ValueAlreadyConsumed, Error::ValueAccessContention] {
        let copied = error;
        assert_eq!(copied, error);
        assert_eq!(Err::<(), _>(copied), Err(error));
    }
    assert_ne!(Error::ValueAlreadyConsumed, Error::ValueAccessContention);
}

#[test]
fn availability_errors_are_distinct_and_have_no_source() {
    let consumed = Error::ValueAlreadyConsumed;
    let contended = Error::ValueAccessContention;
    assert!(consumed.source().is_none());
    assert!(contended.source().is_none());
    assert_ne!(consumed.to_string(), contended.to_string());
    assert_eq!(
        consumed.to_string(),
        "Error: `SystasisContainer` value has already been consumed."
    );
    assert_eq!(
        contended.to_string(),
        "Error: `SystasisContainer` value access is contended."
    );
}

#[test]
fn error_has_only_consumption_and_contention() {
    fn classify(error: Error) -> bool {
        match error {
            Error::ValueAlreadyConsumed => true,
            Error::ValueAccessContention => false,
        }
    }
    assert!(classify(Error::ValueAlreadyConsumed));
    assert!(!classify(Error::ValueAccessContention));
}
