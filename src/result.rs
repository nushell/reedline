use std::fmt::Display;

/// non-public (for now)
#[derive(Debug)]
pub enum ReedlineErrorVariants {
    // todo: we should probably be more specific here
    #[cfg(feature = "_sqlite")]
    /// Error within history database
    HistoryDatabaseError(String),

    /// Error within history
    OtherHistoryError(&'static str),

    /// History does not support a feature
    HistoryFeatureUnsupported {
        /// Custom display name for the history
        history: &'static str,

        /// Unsupported feature
        feature: &'static str,
    },

    /// I/O error
    IOError(std::io::Error),
}

impl Display for ReedlineErrorVariants {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            #[cfg(feature = "_sqlite")]
            Self::HistoryDatabaseError(msg) => {
                write!(f, "error within history database: {msg}")
            }
            Self::HistoryFeatureUnsupported { history, feature } => {
                write!(
                    f,
                    "the history {history} does not support feature {feature}"
                )
            }
            Self::OtherHistoryError(msg) => {
                write!(f, "error in Reedline history: {msg}")
            }
            Self::IOError(msg) => {
                write!(f, "I/O error: {msg}")
            }
        }
    }
}
impl std::error::Error for ReedlineErrorVariants {}

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

impl From<ReedlineError> for std::io::Error {
    fn from(err: ReedlineError) -> Self {
        match err.0 {
            ReedlineErrorVariants::IOError(io) => io,
            other => std::io::Error::other(ReedlineError(other)),
        }
    }
}
