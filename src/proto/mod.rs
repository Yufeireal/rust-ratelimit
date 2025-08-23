// Generated protobuf types and gRPC service definitions
pub mod generated;

// Re-export the main types for easy access
pub use generated::envoy::service::ratelimit::v3::{
    RateLimitRequest, RateLimitResponse, RateLimitDescriptor, RateLimitDescriptorEntry,
    rate_limit_response::{DescriptorStatus, RateLimit, Code as ResponseCode},
    rate_limit_service_server::{RateLimitService, RateLimitServiceServer},
    rate_limit_response,
};