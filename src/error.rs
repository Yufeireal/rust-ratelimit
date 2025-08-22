use thiserror::Error;

/// Result type for rate limit operations
pub type Result<T> = std::result::Result<T, RateLimitError>;

/// Errors that can occur in the rate limit service
#[derive(Error, Debug)]
pub enum RateLimitError {
    #[error("Redis error: {0}")]
    Redis(#[from] redis::RedisError),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Service error: {0}")]
    Service(String),

    #[error("Cache error: {0}")]
    Cache(String),

    #[error("Invalid descriptor: {0}")]
    InvalidDescriptor(String),

    #[error("Domain not found: {0}")]
    DomainNotFound(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("gRPC error: {0}")]
    Grpc(#[from] tonic::Status),
}