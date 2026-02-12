use thiserror::Error;

#[derive(Debug, Error)]
pub enum DromeError {
    #[error("{0}")]
    Message(String),

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Json(#[from] serde_json::Error),

    #[error(transparent)]
    Zip(#[from] zip::result::ZipError),
}

pub type Result<T> = std::result::Result<T, DromeError>;

impl From<DromeError> for String {
    fn from(value: DromeError) -> Self {
        value.to_string()
    }
}
