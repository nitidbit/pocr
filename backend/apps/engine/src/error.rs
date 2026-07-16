use thiserror::Error;

pub type EngineResult<T> = Result<T, EngineError>;

#[derive(Debug, Error)]
pub enum EngineError {
    #[error("unknown error: {0}")]
    Unknown(String),
}