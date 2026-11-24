use thiserror::Error;

/// Errors shared by the domain and the adapters that implement its ports.
#[derive(Debug, Error)]
pub enum DomainError {
    #[error("{entity} not found: {id}")]
    NotFound { entity: &'static str, id: String },

    #[error("invalid input: {0}")]
    Invalid(String),

    #[error("storage failure")]
    Storage(#[source] Box<dyn std::error::Error + Send + Sync>),
}

impl DomainError {
    /// Wraps an adapter error (database, file system…) as a storage failure.
    pub fn storage(err: impl std::error::Error + Send + Sync + 'static) -> Self {
        Self::Storage(Box::new(err))
    }
}

pub type Result<T, E = DomainError> = std::result::Result<T, E>;
