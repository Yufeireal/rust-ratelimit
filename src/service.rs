use std::sync::Arc;
use tokio::sync::RwLock;

use crate::{
    cache::{RateLimitDescriptor, RateLimitRequest, ResponseCode},
    config::CompiledRateLimitConfig,
    limiter::{RateLimiter, RateLimitResponse},
    metrics::Metrics,
};

// Simplified protobuf-like structures for this implementation
// In a production system, these would be generated from .proto files

#[derive(Debug, Clone)]
pub struct GrpcRateLimitRequest {
    pub domain: String,
    pub descriptors: Vec<GrpcRateLimitDescriptor>,
    pub hits_addend: u32,
}

#[derive(Debug, Clone)]
pub struct GrpcRateLimitDescriptor {
    pub entries: Vec<GrpcRateLimitDescriptorEntry>,
}

#[derive(Debug, Clone)]
pub struct GrpcRateLimitDescriptorEntry {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone)]
pub struct GrpcRateLimitResponse {
    pub overall_code: i32,
    pub statuses: Vec<GrpcDescriptorStatus>,
}

#[derive(Debug, Clone)]
pub struct GrpcDescriptorStatus {
    pub code: i32,
    pub current_limit: Option<GrpcRateLimit>,
    pub limit_remaining: u32,
    pub duration_until_reset_secs: u64,
}

#[derive(Debug, Clone)]
pub struct GrpcRateLimit {
    pub requests_per_unit: u32,
    pub unit: i32,
}

// Remove the invalid import since we defined our own types

/// gRPC service implementation for rate limiting
pub struct RateLimitService {
    limiter: Arc<RwLock<RateLimiter>>,
    metrics: Arc<Metrics>,
}

impl RateLimitService {
    /// Create a new rate limit service
    pub fn new(limiter: RateLimiter, metrics: Arc<Metrics>) -> Self {
        Self {
            limiter: Arc::new(RwLock::new(limiter)),
            metrics,
        }
    }

    /// Add a configuration to the service
    pub async fn add_config(&self, config: CompiledRateLimitConfig) -> crate::error::Result<()> {
        let mut limiter = self.limiter.write().await;
        limiter.add_config(config);
        self.metrics.record_config_load_success();
        Ok(())
    }

    /// Remove a configuration from the service
    pub async fn remove_config(&self, domain: &str) -> crate::error::Result<()> {
        let mut limiter = self.limiter.write().await;
        limiter.remove_config(domain);
        Ok(())
    }

    /// Health check for the service
    pub async fn health_check(&self) -> crate::error::Result<()> {
        let limiter = self.limiter.read().await;
        limiter.health_check().await
    }

    /// Convert internal response code to gRPC response code
    fn convert_response_code(code: ResponseCode) -> i32 {
        match code {
            ResponseCode::Ok => 1,
            ResponseCode::OverLimit => 2,
        }
    }

    /// Convert internal response to gRPC response
    fn convert_response(response: RateLimitResponse) -> GrpcRateLimitResponse {
        let overall_code = Self::convert_response_code(response.overall_code);
        
        let statuses = response
            .statuses
            .into_iter()
            .map(|status| GrpcDescriptorStatus {
                code: Self::convert_response_code(status.code),
                current_limit: status.current_limit.map(|limit| GrpcRateLimit {
                    requests_per_unit: limit.requests_per_unit,
                    unit: match limit.unit {
                        crate::utils::Unit::Second => 1,
                        crate::utils::Unit::Minute => 2,
                        crate::utils::Unit::Hour => 3,
                        crate::utils::Unit::Day => 4,
                    },
                }),
                limit_remaining: status.limit_remaining,
                duration_until_reset_secs: status.duration_until_reset_secs,
            })
            .collect();

        GrpcRateLimitResponse {
            overall_code,
            statuses,
        }
    }
}

// The gRPC implementation is now in main.rs using the generated protobuf types

impl RateLimitService {
    /// Process a rate limit request (for non-gRPC callers)
    pub async fn should_rate_limit_direct(
        &self,
        request: GrpcRateLimitRequest,
    ) -> crate::error::Result<GrpcRateLimitResponse> {
        let timer = self.metrics.start_request_timer();
        let req = request;

        // Convert gRPC request to internal request
        let internal_request = RateLimitRequest {
            domain: req.domain.clone(),
            descriptors: req
                .descriptors
                .into_iter()
                .map(|desc| RateLimitDescriptor {
                    entries: desc
                        .entries
                        .into_iter()
                        .map(|entry| (entry.key, entry.value))
                        .collect(),
                })
                .collect(),
            hits_addend: req.hits_addend,
        };

        // Record metrics
        for descriptor in &internal_request.descriptors {
            let descriptor_key = if descriptor.entries.is_empty() {
                "unknown".to_string()
            } else {
                descriptor.entries[0].0.clone()
            };
            self.metrics.record_total_request(&req.domain, &descriptor_key);
        }

        // Process the request
        let result = {
            let limiter = self.limiter.read().await;
            limiter.should_rate_limit(&internal_request).await
        };

        drop(timer);

        match result {
            Ok(response) => {
                // Record additional metrics based on response
                for (i, status) in response.statuses.iter().enumerate() {
                    let descriptor_key = if internal_request.descriptors[i].entries.is_empty() {
                        "unknown".to_string()
                    } else {
                        internal_request.descriptors[i].entries[0].0.clone()
                    };

                    match status.code {
                        ResponseCode::Ok => {
                            self.metrics.record_within_limit_request(&req.domain, &descriptor_key);
                        }
                        ResponseCode::OverLimit => {
                            self.metrics.record_over_limit_request(&req.domain, &descriptor_key);
                        }
                    }
                }

                let grpc_response = Self::convert_response(response);
                Ok(grpc_response)
            }
            Err(e) => {
                self.metrics.record_config_load_error();
                Err(e)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        cache::RedisRateLimitCache,
        config::{RateLimit, RateLimitConfig, RateLimitDescriptor as ConfigDescriptor, RateLimitUnit},
        redis::{RedisClientPool, RedisConfig},
    };

    async fn create_test_service() -> RateLimitService {
        let redis_config = RedisConfig::default();
        let redis_pool = RedisClientPool::new_single(redis_config).await.unwrap();
        let cache = RedisRateLimitCache::new(redis_pool, 1000, 0.8, "test".to_string());
        let limiter = RateLimiter::new(Box::new(cache));
        let metrics = Arc::new(Metrics::new().unwrap());
        
        RateLimitService::new(limiter, metrics)
    }

    #[tokio::test]
    async fn test_service_creation() {
        let _service = create_test_service().await;
    }

    #[tokio::test]
    async fn test_config_management() {
        let service = create_test_service().await;

        let config = RateLimitConfig {
            domain: "test".to_string(),
            descriptors: vec![ConfigDescriptor {
                key: "key1".to_string(),
                value: Some("value1".to_string()),
                rate_limit: Some(RateLimit {
                    requests_per_unit: 100,
                    unit: RateLimitUnit::Second,
                    unlimited: None,
                    name: None,
                }),
                shadow_mode: None,
                descriptors: None,
            }],
        };

        let compiled_config = crate::config::CompiledRateLimitConfig::compile(config).unwrap();
        service.add_config(compiled_config).await.unwrap();
    }

    #[tokio::test]
    async fn test_should_rate_limit_empty_domain() {
        let service = create_test_service().await;

        let request = GrpcRateLimitRequest {
            domain: "".to_string(),
            descriptors: vec![GrpcRateLimitDescriptor {
                entries: vec![GrpcRateLimitDescriptorEntry {
                    key: "key1".to_string(),
                    value: "value1".to_string(),
                }],
            }],
            hits_addend: 1,
        };

        let result = service.should_rate_limit_direct(request).await;
        assert!(result.is_err());
        
        match result.unwrap_err() {
            crate::error::RateLimitError::Service(msg) => {
                assert!(msg.contains("domain must not be empty"));
            }
            _ => panic!("Expected service error"),
        }
    }
}