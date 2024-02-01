use std::fmt::Display;
use thiserror::Error;

/// non-public (for now)
#[derive(Error, Debug)]
pub enum ReedlineErrorVariants {
    // todo: we should probably be more specific here
    #[cfg(any(feature = "sqlite", feature = "sqlite-dynlib"))]
    /// Error within history database
    #[error("error within history database: {0}")]
    HistoryDatabaseError(String),

    /// Error within history
    #[error("error in Reedline history: {0}")]
    OtherHistoryError(&'static str),

    /// History does not support a feature
    #[error("the history {history} does not support feature {feature}")]
    HistoryFeatureUnsupported {
        /// Custom display name for the history
        history: &'static str,

        /// Unsupported feature
        feature: &'static str,
    },

    /// I/O error
    #[error("I/O error: {0}")]
    IOError(std::io::Error),
}

/// separate struct to not expose anything to the public (for now)
#[derive(Debug)]
pub struct ReedlineError(pub ReedlineErrorVariants);

impl From<std::io::Error> for ReedlineError {
    fn from(err: std::io::Error) -> Self {
        Self(ReedlineErrorVariants::IOError(err))
    }
}

impl Display for ReedlineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}
impl std::error::Error for ReedlineError {}

/// Standard [`std::result::Result`], with [`ReedlineError`] as the error variant
pub type Result<T> = std::result::Result<T, ReedlineError>;
