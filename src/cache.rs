use async_trait::async_trait;
use moka::{future::Cache, Expiry};
use std::{collections::HashMap, sync::Arc, time::{Duration, Instant}};
use tokio::sync::Mutex;

use crate::{
    config::{CompiledRateLimit},
    error::{RateLimitError, Result},
    redis::RedisClientPool,
    utils::{generate_cache_key, get_hits_addend, TimeSource, Unit},
};

/// Response status for a single descriptor
#[derive(Debug, Clone)]
pub struct DescriptorStatus {
    pub code: ResponseCode,
    pub current_limit: Option<RateLimit>,
    pub limit_remaining: u32,
    pub duration_until_reset_secs: u64,
}

/// Response codes for rate limiting
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResponseCode {
    Ok,
    OverLimit,
}

/// Rate limit information
#[derive(Debug, Clone)]
pub struct RateLimit {
    pub requests_per_unit: u32,
    pub unit: Unit,
}

/// Rate limit request descriptor
#[derive(Debug, Clone)]
pub struct RateLimitDescriptor {
    pub entries: Vec<(String, String)>,
}

/// Rate limit request
#[derive(Debug, Clone)]
pub struct RateLimitRequest {
    pub domain: String,
    pub descriptors: Vec<RateLimitDescriptor>,
    pub hits_addend: u32,
}

/// Main trait for rate limit caching
#[async_trait]
pub trait RateLimitCache: Send + Sync {
    /// Perform rate limiting check for the given request
    async fn do_limit(&self, request: &RateLimitRequest) -> Result<Vec<DescriptorStatus>>;
    
    /// Health check for the cache
    async fn health_check(&self) -> Result<()>;
}

/// Redis-based rate limit cache implementation
pub struct RedisRateLimitCache {
    redis_pool: RedisClientPool,
    local_cache: Arc<Cache<String, (Expiration, String)>>,
    time_source: TimeSource,
    near_limit_ratio: f32,
    cache_key_prefix: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Expiration {
    // The value will pass after 
    Duration(Unit),     
}

impl Expiration {
    pub fn as_duration(&self) -> Option<Duration> {
        match self {
            Expiration::Duration(unit) => {
                let seconds = match unit {
                    Unit::Second => 1,
                    Unit::Minute => 60,
                    Unit::Hour =>  3600,
                    Unit::Day => 86400,
                };
                Some(Duration::from_secs(seconds))
            }
        }
    }
}

pub struct MyExpiry;

impl Expiry<String, (Expiration, String)> for MyExpiry {
    fn expire_after_create(
        &self,
        _key: &String,
        value: &(Expiration, String),
        _current_time: Instant,
    ) -> Option<Duration> {
        let duration = value.0.as_duration();
        duration
    }
}


impl RedisRateLimitCache {
    /// Create a new Redis-based rate limit cache
    pub fn new(
        redis_pool: RedisClientPool,
        local_cache_size: u64,
        near_limit_ratio: f32,
        cache_key_prefix: String,
    ) -> Self {
        let local_cache = Cache::builder()
            .max_capacity(local_cache_size)
            .expire_after(MyExpiry)
            .build();

        Self {
            redis_pool,
            local_cache: Arc::new(local_cache),
            time_source: TimeSource::new(),
            near_limit_ratio,
            cache_key_prefix,
        }
    }

    /// Generate cache keys for descriptors
    fn generate_cache_keys(
        &self,
        request: &RateLimitRequest,
        limits: &[Option<&CompiledRateLimit>],
    ) -> Vec<Option<CacheKey>> {
        limits
            .iter()
            .zip(&request.descriptors)
            .map(|(limit, descriptor)| {
                limit.map(|l| {
                    let descriptors: Vec<(&str, &str)> = descriptor
                        .entries
                        .iter()
                        .map(|(k, v)| (k.as_str(), v.as_str()))
                        .collect();

                    let key = if self.cache_key_prefix.is_empty() {
                        generate_cache_key(&request.domain, &descriptors, l.unit, &self.time_source)
                    } else {
                        format!(
                            "{}:{}",
                            self.cache_key_prefix,
                            generate_cache_key(&request.domain, &descriptors, l.unit, &self.time_source)
                        )
                    };

                    CacheKey {
                        key,
                        per_second: l.unit.is_per_second(),
                    }
                })
            })
            .collect()
    }

    /// Check if a key is over limit in local cache
    async fn is_over_limit_with_local_cache(&self, key: &str) -> bool {
        self.local_cache.get(key).await.is_some()
    }

    /// Add a key to the local cache as over-limit
    async fn add_to_local_cache(&self, key: &str, unit: &Unit) {
        self.local_cache.insert(key.into(), (Expiration::Duration(unit.clone()), "".into())).await
    }

    /// Generate response descriptor status
    fn generate_response_descriptor_status(
        &self,
        code: ResponseCode,
        limit: Option<&CompiledRateLimit>,
        limit_remaining: u32,
    ) -> DescriptorStatus {
        let current_limit = limit.map(|l| RateLimit {
            requests_per_unit: l.requests_per_unit,
            unit: l.unit,
        });

        let duration_until_reset_secs = if let Some(l) = limit {
            crate::utils::calculate_reset(&l.unit, &self.time_source).as_secs()
        } else {
            0
        };

        DescriptorStatus {
            code,
            current_limit,
            limit_remaining,
            duration_until_reset_secs,
        }
    }
}

#[async_trait]
impl RateLimitCache for RedisRateLimitCache {
    async fn do_limit(&self, request: &RateLimitRequest) -> Result<Vec<DescriptorStatus>> {
        if request.descriptors.is_empty() {
            return Err(RateLimitError::Service(
                "Rate limit descriptor list must not be empty".to_string(),
            ));
        }

        // For this implementation, we need the compiled limits to be passed in
        // In a real implementation, these would come from the configuration
        let limits: Vec<Option<&CompiledRateLimit>> = vec![None; request.descriptors.len()];

        let cache_keys = self.generate_cache_keys(request, &limits);
        let hits_addend = get_hits_addend(request.hits_addend);

        let mut results = Vec::new();
        let mut over_limit_local_cache = vec![false; request.descriptors.len()];

        // Check local cache for over-limit keys
        for (i, cache_key) in cache_keys.iter().enumerate() {
            if let Some(key) = cache_key {
                if self.is_over_limit_with_local_cache(&key.key).await {
                    over_limit_local_cache[i] = true;
                }
            }
        }

        // Prepare Redis operations
        let mut redis_operations = Vec::new();
        let mut operation_indices = Vec::new();

        for (i, (cache_key, limit)) in cache_keys.iter().zip(&limits).enumerate() {
            if let (Some(key), Some(limit)) = (cache_key, limit) {
                if !over_limit_local_cache[i] && !limit.unlimited {
                    redis_operations.push((
                        key.key.clone(),
                        hits_addend,
                        limit.unit.to_seconds(),
                    ));
                    operation_indices.push(i);
                }
            }
        }

        // Execute Redis operations based on per-second vs other units
        let mut per_second_ops = Vec::new();
        let mut other_ops = Vec::new();
        let mut per_second_indices = Vec::new();
        let mut other_indices = Vec::new();

        for (op_idx, (key, increment, expire)) in redis_operations.iter().enumerate() {
            let cache_key = cache_keys[operation_indices[op_idx]].as_ref().unwrap();
            if cache_key.per_second {
                per_second_ops.push((key.clone(), *increment, *expire));
                per_second_indices.push(operation_indices[op_idx]);
            } else {
                other_ops.push((key.clone(), *increment, *expire));
                other_indices.push(operation_indices[op_idx]);
            }
        }

        // Execute operations
        let per_second_results = if !per_second_ops.is_empty() {
            let client = self.redis_pool.get_client(true);
            client.pipeline_increment_and_expire(per_second_ops).await?
        } else {
            Vec::new()
        };

        let other_results = if !other_ops.is_empty() {
            let client = self.redis_pool.get_client(false);
            client.pipeline_increment_and_expire(other_ops).await?
        } else {
            Vec::new()
        };

        // Combine results
        let mut redis_result_map = HashMap::new();
        for (i, &idx) in per_second_indices.iter().enumerate() {
            redis_result_map.insert(idx, per_second_results[i]);
        }
        for (i, &idx) in other_indices.iter().enumerate() {
            redis_result_map.insert(idx, other_results[i]);
        }

        // Generate response statuses
        for (i, (cache_key, limit)) in cache_keys.iter().zip(&limits).enumerate() {
            let status = if let (Some(_key), Some(limit)) = (cache_key, limit) {
                if limit.unlimited {
                    // Unlimited rate limit
                    self.generate_response_descriptor_status(ResponseCode::Ok, Some(limit), u32::MAX)
                } else if over_limit_local_cache[i] {
                    // Over limit from local cache
                    self.generate_response_descriptor_status(ResponseCode::OverLimit, Some(limit), 0)
                } else if let Some(&current_count) = redis_result_map.get(&i) {
                    // Check Redis result
                    let over_limit_threshold = limit.requests_per_unit as u64;
                    let is_over_limit = current_count > over_limit_threshold;
                    
                    if is_over_limit && !limit.shadow_mode {
                        // Add to local cache for future requests
                        if let Some(key) = cache_key {
                            self.add_to_local_cache(&key.key, &limit.unit).await;
                        }
                        
                        self.generate_response_descriptor_status(ResponseCode::OverLimit, Some(limit), 0)
                    } else {
                        let remaining = if current_count >= over_limit_threshold {
                            0
                        } else {
                            (over_limit_threshold - current_count) as u32
                        };
                        
                        let code = if limit.shadow_mode && is_over_limit {
                            ResponseCode::Ok  // Shadow mode always returns OK
                        } else if is_over_limit {
                            ResponseCode::OverLimit
                        } else {
                            ResponseCode::Ok
                        };
                        
                        self.generate_response_descriptor_status(code, Some(limit), remaining)
                    }
                } else {
                    // No Redis operation (shouldn't happen)
                    self.generate_response_descriptor_status(ResponseCode::Ok, Some(limit), limit.requests_per_unit)
                }
            } else {
                // No limit configured - allow through
                self.generate_response_descriptor_status(ResponseCode::Ok, None, 0)
            };

            results.push(status);
        }

        Ok(results)
    }

    async fn health_check(&self) -> Result<()> {
        self.redis_pool.health_check().await
    }
}

/// Cache key with metadata
#[derive(Debug, Clone)]
struct CacheKey {
    key: String,
    per_second: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{config::CompiledRateLimit, redis::RedisConfig};

    #[tokio::test]
    async fn test_cache_key_generation() {
        let redis_config = RedisConfig::default();
        let redis_pool = RedisClientPool::new_single(redis_config).await.unwrap();
        let cache = RedisRateLimitCache::new(redis_pool, 1000, 0.8, "test".to_string());

        let request = RateLimitRequest {
            domain: "test_domain".to_string(),
            descriptors: vec![RateLimitDescriptor {
                entries: vec![("key1".to_string(), "value1".to_string())],
            }],
            hits_addend: 1,
        };

        let limit = CompiledRateLimit {
            requests_per_unit: 100,
            unit: Unit::Second,
            unlimited: false,
            shadow_mode: false,
            name: None,
        };
        let limits = vec![Some(&limit)];
        let cache_keys = cache.generate_cache_keys(&request, &limits);
        assert_eq!(cache_keys.len(), 1);
        assert!(cache_keys[0].is_some());
        let cache_key = cache_keys[0].as_ref().unwrap();
        assert!(cache_key.key.contains("test:test_domain:key1_value1:"));
        assert!(cache_key.per_second);
    }
}