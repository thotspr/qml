//! Error types for QML Rust.
//!
//! This module provides comprehensive error handling for all QML operations,
//! using the thiserror crate for ergonomic error handling.

use thiserror::Error;

/// The main error type for QML operations.
///
/// This enum covers all possible errors that can occur during job processing,
/// storage operations, and serialization.
#[derive(Error, Debug, Clone, PartialEq)]
pub enum QmlError {
    /// Job not found error
    #[error("Job not found: {job_id}")]
    JobNotFound { job_id: String },

    /// Serialization/deserialization errors
    #[error("Serialization failed: {message}")]
    SerializationError { message: String },

    /// Storage-related errors
    #[error("Storage error: {message}")]
    StorageError { message: String },

    /// Invalid job state transition
    #[error("Invalid state transition from {from} to {to}")]
    InvalidStateTransition { from: String, to: String },

    /// Invalid job data
    #[error("Invalid job data: {message}")]
    InvalidJobData { message: String },

    /// Queue operation errors
    #[error("Queue operation failed: {message}")]
    QueueError { message: String },

    /// Worker-related errors
    #[error("Worker error: {message}")]
    WorkerError { message: String },

    /// Configuration errors
    #[error("Configuration error: {message}")]
    ConfigurationError { message: String },

    /// Timeout errors
    #[error("Operation timed out: {operation}")]
    TimeoutError { operation: String },

    /// Database migration errors
    #[error("Migration error: {message}")]
    MigrationError { message: String },
}

impl From<serde_json::Error> for QmlError {
    fn from(err: serde_json::Error) -> Self {
        QmlError::SerializationError {
            message: err.to_string(),
        }
    }
}

impl From<uuid::Error> for QmlError {
    fn from(err: uuid::Error) -> Self {
        QmlError::InvalidJobData {
            message: format!("UUID error: {}", err),
        }
    }
}

/// A specialized Result type for QML operations.
pub type Result<T> = std::result::Result<T, QmlError>;
