//! Error contract shared by generated containers.

/// A checked stored-value access could not complete.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Ownership was already transferred out of the container.
    #[error("Error: `AppContainer` value has already been consumed.")]
    ValueAlreadyConsumed,
    /// The required access could not be acquired immediately.
    #[error("Error: `AppContainer` value access is contended.")]
    ValueAccessContention,
}
