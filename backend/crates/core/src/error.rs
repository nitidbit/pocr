use thiserror::Error;

/// Convenience alias for results within the randomization core.
pub type RandomizationResult<T> = Result<T, RandomizationError>;

/// Errors produced by randomization operations.
#[derive(Error, Debug)]
#[non_exhaustive]
pub enum RandomizationError {
    /// The requested block size is not a valid multiple of the ratio sum.
    #[error("invalid block size (size {block_size:?} is not a multiple of ratio sum {ratio_sum:?})")]
    InvalidBlockSize {
        /// The invalid block size that was requested.
        block_size: u32,
        /// The sum of all arm ratios (block size must be a multiple of this).
        ratio_sum: u32,
    },

    /// All assignments in the current block have been consumed.
    #[error("block exhausted")]
    BlockExhausted,

    /// A required field was not present on the randomization request.
    #[error("missing required field: {field}")]
    MissingField {
        /// Name of the missing field.
        field: &'static str,
    },

    /// The requested resource or lookup key was not found.
    #[error("not found")]
    NotFound,

    /// The requested operation is not yet implemented.
    #[error("not implemented: {0}")]
    NotImplemented(String),

    /// An infrastructure/IO error (database, cache, network).
    #[error("infrastructure error: {0}")]
    Infrastructure(String),

    /// Study configuration is invalid (e.g., all arm ratios are zero).
    #[error("invalid configuration: {0}")]
    InvalidConfiguration(String),

    /// The study is not active, so this environment may not randomize into it.
    #[error("study is not active")]
    StudyNotActive,

    /// The study is closed; no environment may randomize into it.
    #[error("study is closed")]
    StudyClosed,

    /// An unclassified error occurred.
    #[error("unknown randomization error")]
    Unknown,
}