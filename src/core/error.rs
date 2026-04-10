use std::path::PathBuf;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Config error: {0}")]
    Config(String),

    #[error("API key not found")]
    ApiKeyNotFound,

    #[error("API error: {0}")]
    Api(String),

    #[error("Cancelled by user")]
    Cancelled,

    #[error("Max retries exceeded")]
    MaxRetriesExceeded,

    #[error("Plan validation failed after {attempts} attempts")]
    PlanValidationFailed { attempts: usize },

    #[error("Git error: {0}")]
    Git(String),

    #[error("Merge conflict in files: {files:?}")]
    MergeConflict { files: Vec<PathBuf> },

    #[error("Rate limited")]
    RateLimited,

    #[error("External file change detected: {0}")]
    ExternalFileChange(PathBuf),

    #[error("Task failed: {0}")]
    TaskFailed(String),

    #[error("{0}")]
    Other(String),
}

impl From<anyhow::Error> for Error {
    fn from(e: anyhow::Error) -> Self {
        Error::Other(e.to_string())
    }
}
