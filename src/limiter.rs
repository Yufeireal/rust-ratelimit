use std::collections::HashMap;
use crate::{
    cache::{DescriptorStatus, RateLimitCache, RateLimitRequest, ResponseCode},
    config::CompiledRateLimitConfig,
    error::{Result, RateLimitError},
};

/// Main rate limiter that coordinates configuration and caching
pub struct RateLimiter {
    configurations: HashMap<String, CompiledRateLimitConfig>,
    cache: Box<dyn RateLimitCache>,
}

impl RateLimiter {
    /// Create a new rate limiter with the given cache implementation
    pub fn new(cache: Box<dyn RateLimitCache>) -> Self {
        Self {
            configurations: HashMap::new(),
            cache,
        }
    }

    /// Add a configuration for a domain
    pub fn add_config(&mut self, config: CompiledRateLimitConfig) {
        let domain = config.domain().to_string();
        self.configurations.insert(domain, config);
    }

    /// Remove a configuration for a domain
    pub fn remove_config(&mut self, domain: &str) -> Option<CompiledRateLimitConfig> {
        self.configurations.remove(domain)
    }

    /// Get configuration for a domain
    pub fn get_config(&self, domain: &str) -> Option<&CompiledRateLimitConfig> {
        self.configurations.get(domain)
    }

    /// Check if rate limiting should be applied to the request
    pub async fn should_rate_limit(&self, request: &RateLimitRequest) -> Result<RateLimitResponse> {
        // Validate request
        if request.domain.is_empty() {
            return Err(RateLimitError::Service(
                "Rate limit domain must not be empty".to_string(),
            ));
        }

        if request.descriptors.is_empty() {
            return Err(RateLimitError::Service(
                "Rate limit descriptor list must not be empty".to_string(),
            ));
        }

        // Get configuration for domain
        let config = self
            .get_config(&request.domain)
            .ok_or_else(|| RateLimitError::DomainNotFound(request.domain.clone()))?;

        // Find limits for each descriptor
        let mut enriched_request = EnrichedRateLimitRequest {
            domain: request.domain.clone(),
            descriptors: Vec::new(),
            hits_addend: request.hits_addend,
        };

        for descriptor in &request.descriptors {
            // Convert descriptor entries to the format expected by config lookup
            let descriptor_pairs: Vec<(&str, &str)> = descriptor
                .entries
                .iter()
                .map(|(k, v)| (k.as_str(), v.as_str()))
                .collect();

            let limit = config.find_limit(&descriptor_pairs);
            
            enriched_request.descriptors.push(EnrichedDescriptor {
                entries: descriptor.entries.clone(),
                limit: limit.cloned(),
            });
        }

        // Delegate to cache for actual rate limiting
        let statuses = self.do_limit_with_config(&enriched_request).await?;

        // Determine overall response code
        let overall_code = if statuses.iter().any(|s| s.code == ResponseCode::OverLimit) {
            ResponseCode::OverLimit
        } else {
            ResponseCode::Ok
        };

        Ok(RateLimitResponse {
            overall_code,
            statuses,
        })
    }

    /// Perform rate limiting with configuration context
    async fn do_limit_with_config(
        &self,
        request: &EnrichedRateLimitRequest,
    ) -> Result<Vec<DescriptorStatus>> {
        // This is a simplified implementation
        // In a complete implementation, we would pass the limits to the cache
        let base_request = RateLimitRequest {
            domain: request.domain.clone(),
            descriptors: request
                .descriptors
                .iter()
                .map(|d| crate::cache::RateLimitDescriptor {
                    entries: d.entries.clone(),
                })
                .collect(),
            hits_addend: request.hits_addend,
        };

        self.cache.do_limit(&base_request).await
    }

    /// Health check for the limiter
    pub async fn health_check(&self) -> Result<()> {
        self.cache.health_check().await
    }
}

/// Response for a rate limit check
#[derive(Debug)]
pub struct RateLimitResponse {
    pub overall_code: ResponseCode,
    pub statuses: Vec<DescriptorStatus>,
}

/// Internal enriched request with resolved limits
struct EnrichedRateLimitRequest {
    pub domain: String,
    pub descriptors: Vec<EnrichedDescriptor>,
    pub hits_addend: u32,
}

/// Internal enriched descriptor with resolved limit
struct EnrichedDescriptor {
    pub entries: Vec<(String, String)>,
    pub limit: Option<crate::config::CompiledRateLimit>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        cache::{RedisRateLimitCache, RateLimitDescriptor},
        config::{CompiledRateLimit, CompiledRateLimitConfig, RateLimit, RateLimitConfig, RateLimitUnit},
        redis::{RedisClientPool, RedisConfig},
        utils::Unit,
    };

    async fn create_test_limiter() -> RateLimiter {
        let redis_config = RedisConfig::default();
        let redis_pool = RedisClientPool::new_single(redis_config).await.unwrap();
        let cache = RedisRateLimitCache::new(redis_pool, 1000, 0.8, "test".to_string());
        
        RateLimiter::new(Box::new(cache))
    }

    #[tokio::test]
    async fn test_limiter_creation() {
        let _limiter = create_test_limiter().await;
    }

    #[tokio::test]
    async fn test_config_management() {
        let mut limiter = create_test_limiter().await;

        let config = RateLimitConfig {
            domain: "test".to_string(),
            descriptors: vec![crate::config::RateLimitDescriptor {
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

        let compiled_config = CompiledRateLimitConfig::compile(config).unwrap();
        limiter.add_config(compiled_config);

        assert!(limiter.get_config("test").is_some());
        assert!(limiter.get_config("nonexistent").is_none());
    }

    #[tokio::test]
    async fn test_empty_domain_request() {
        let limiter = create_test_limiter().await;

        let request = RateLimitRequest {
            domain: "".to_string(),
            descriptors: vec![RateLimitDescriptor {
                entries: vec![("key1".to_string(), "value1".to_string())],
            }],
            hits_addend: 1,
        };

        let result = limiter.should_rate_limit(&request).await;
        assert!(result.is_err());
        
        if let Err(RateLimitError::Service(msg)) = result {
            assert!(msg.contains("domain must not be empty"));
        } else {
            panic!("Expected service error for empty domain");
        }
    }

    #[tokio::test]
    async fn test_empty_descriptors_request() {
        let limiter = create_test_limiter().await;

        let request = RateLimitRequest {
            domain: "test".to_string(),
            descriptors: vec![],
            hits_addend: 1,
        };

        let result = limiter.should_rate_limit(&request).await;
        assert!(result.is_err());
        
        if let Err(RateLimitError::Service(msg)) = result {
            assert!(msg.contains("descriptor list must not be empty"));
        } else {
            panic!("Expected service error for empty descriptors");
        }
    }
}