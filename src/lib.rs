//! Rust Rate Limit Service
//! 
//! A Rust implementation of the Envoy rate limit service with Redis backend.
//! This service provides generic rate limiting capabilities for applications
//! using domain-based configuration and descriptor matching.

pub mod cache;
pub mod config;
pub mod error;
pub mod limiter;
pub mod metrics;
pub mod proto;
pub mod redis;
pub mod service;
pub mod utils;

// Re-export main types
pub use cache::RateLimitCache;
pub use config::{RateLimitConfig, RateLimitDescriptor};
pub use error::{RateLimitError, Result};
pub use service::RateLimitService;