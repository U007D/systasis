//! Public error behavior in both runtime configurations.
use std::error::Error as _;
use systasis::app_container::Error;

#[test]
fn availability_errors_are_distinct_and_have_no_source() {
    let consumed = Error::ValueAlreadyConsumed;
    let contended = Error::ValueAccessContention;
    assert!(consumed.source().is_none());
    assert!(contended.source().is_none());
    assert_ne!(consumed.to_string(), contended.to_string());
}

#[cfg(not(feature = "std"))]
#[test]
fn no_std_error_has_only_consumption_and_contention() {
    fn classify(error: Error) -> bool {
        match error {
            Error::ValueAlreadyConsumed => true,
            Error::ValueAccessContention => false,
        }
    }
    assert!(classify(Error::ValueAlreadyConsumed));
    assert!(!classify(Error::ValueAccessContention));
}
