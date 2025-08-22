use rust_ratelimit::{
    cache::{RedisRateLimitCache, RateLimitDescriptor, RateLimitRequest, ResponseCode},
    config::{CompiledRateLimitConfig, RateLimit, RateLimitConfig, RateLimitDescriptor as ConfigDescriptor, RateLimitUnit},
    limiter::RateLimiter,
    redis::{RedisClientPool, RedisConfig},
};
use std::time::Duration;
use tokio::time::sleep;

#[tokio::test]
async fn test_basic_rate_limiting() {
    // This test would require a real Redis instance
    // For now, we'll test the configuration and structure
    
    let config = RateLimitConfig {
        domain: "test_domain".to_string(),
        descriptors: vec![
            ConfigDescriptor {
                key: "database".to_string(),
                value: Some("users".to_string()),
                rate_limit: Some(RateLimit {
                    requests_per_unit: 5,
                    unit: RateLimitUnit::Second,
                    unlimited: None,
                    name: None,
                }),
                shadow_mode: None,
                descriptors: None,
            },
            ConfigDescriptor {
                key: "api".to_string(),
                value: Some("read".to_string()),
                rate_limit: Some(RateLimit {
                    requests_per_unit: 100,
                    unit: RateLimitUnit::Minute,
                    unlimited: None,
                    name: None,
                }),
                shadow_mode: None,
                descriptors: None,
            },
        ],
    };

    let compiled_config = CompiledRateLimitConfig::compile(config).unwrap();
    assert_eq!(compiled_config.domain(), "test_domain");

    // Test limit lookup
    let limit = compiled_config.find_limit(&[("database", "users")]);
    assert!(limit.is_some());
    assert_eq!(limit.unwrap().requests_per_unit, 5);

    let limit = compiled_config.find_limit(&[("api", "read")]);
    assert!(limit.is_some());
    assert_eq!(limit.unwrap().requests_per_unit, 100);

    let limit = compiled_config.find_limit(&[("nonexistent", "key")]);
    assert!(limit.is_none());
}

#[tokio::test]
async fn test_nested_descriptors() {
    let config = RateLimitConfig {
        domain: "messaging".to_string(),
        descriptors: vec![
            ConfigDescriptor {
                key: "message_type".to_string(),
                value: Some("marketing".to_string()),
                rate_limit: None,
                shadow_mode: None,
                descriptors: Some(vec![
                    ConfigDescriptor {
                        key: "to_number".to_string(),
                        value: None, // Match any value
                        rate_limit: Some(RateLimit {
                            requests_per_unit: 5,
                            unit: RateLimitUnit::Day,
                            unlimited: None,
                            name: None,
                        }),
                        shadow_mode: None,
                        descriptors: None,
                    }
                ]),
            },
            ConfigDescriptor {
                key: "to_number".to_string(),
                value: None,
                rate_limit: Some(RateLimit {
                    requests_per_unit: 100,
                    unit: RateLimitUnit::Day,
                    unlimited: None,
                    name: None,
                }),
                shadow_mode: None,
                descriptors: None,
            },
        ],
    };

    let compiled_config = CompiledRateLimitConfig::compile(config).unwrap();

    // Test the flat descriptor lookup (should match "to_number" key with any value)
    let limit = compiled_config.find_limit(&[("to_number", "")]);
    assert!(limit.is_some());
    assert_eq!(limit.unwrap().requests_per_unit, 100);

    // Test nested descriptor lookup (should match "message_type_marketing:to_number" path)
    let limit = compiled_config.find_limit(&[("message_type", "marketing"), ("to_number", "")]);
    assert!(limit.is_some());
    assert_eq!(limit.unwrap().requests_per_unit, 5);
}

#[tokio::test]
async fn test_shadow_mode() {
    let config = RateLimitConfig {
        domain: "test".to_string(),
        descriptors: vec![
            ConfigDescriptor {
                key: "user".to_string(),
                value: Some("test_user".to_string()),
                rate_limit: Some(RateLimit {
                    requests_per_unit: 1,
                    unit: RateLimitUnit::Second,
                    unlimited: None,
                    name: None,
                }),
                shadow_mode: Some(true),
                descriptors: None,
            },
        ],
    };

    let compiled_config = CompiledRateLimitConfig::compile(config).unwrap();
    let limit = compiled_config.find_limit(&[("user", "test_user")]);
    
    assert!(limit.is_some());
    assert!(limit.unwrap().shadow_mode);
}

#[tokio::test]
async fn test_unlimited_rate_limit() {
    let config = RateLimitConfig {
        domain: "internal".to_string(),
        descriptors: vec![
            ConfigDescriptor {
                key: "service".to_string(),
                value: Some("health_check".to_string()),
                rate_limit: Some(RateLimit {
                    requests_per_unit: 0, // Not used for unlimited
                    unit: RateLimitUnit::Second,
                    unlimited: Some(true),
                    name: None,
                }),
                shadow_mode: None,
                descriptors: None,
            },
        ],
    };

    let compiled_config = CompiledRateLimitConfig::compile(config).unwrap();
    let limit = compiled_config.find_limit(&[("service", "health_check")]);
    
    assert!(limit.is_some());
    assert!(limit.unwrap().unlimited);
}

// Mock tests that would work with an actual Redis instance
#[tokio::test]
async fn test_cache_key_generation() {
    // Test cache key generation without requiring Redis
    use rust_ratelimit::utils::{generate_cache_key, TimeSource, Unit};
    
    let time_source = TimeSource::new();
    let descriptors = vec![("database", "users"), ("action", "read")];
    
    let key = generate_cache_key("test_domain", &descriptors, Unit::Second, &time_source);
    
    assert!(key.starts_with("test_domain:database_users:action_read:"));
    assert!(key.len() > "test_domain:database_users:action_read:".len());
}

#[tokio::test]
async fn test_time_windows() {
    use rust_ratelimit::utils::{TimeSource, Unit, generate_cache_key};
    
    let time_source = TimeSource::new();
    let descriptors = vec![("test", "key")];
    
    // Generate keys for different units
    let second_key = generate_cache_key("domain", &descriptors, Unit::Second, &time_source);
    let minute_key = generate_cache_key("domain", &descriptors, Unit::Minute, &time_source);
    let hour_key = generate_cache_key("domain", &descriptors, Unit::Hour, &time_source);
    let day_key = generate_cache_key("domain", &descriptors, Unit::Day, &time_source);
    
    // Keys should be different due to different time windows
    assert_ne!(second_key, minute_key);
    assert_ne!(minute_key, hour_key);
    assert_ne!(hour_key, day_key);
    
    // But if generated at the same time with same unit, should be same
    let second_key2 = generate_cache_key("domain", &descriptors, Unit::Second, &time_source);
    assert_eq!(second_key, second_key2);
}

#[tokio::test]
async fn test_hits_addend() {
    use rust_ratelimit::utils::get_hits_addend;
    
    assert_eq!(get_hits_addend(0), 1);
    assert_eq!(get_hits_addend(1), 1);
    assert_eq!(get_hits_addend(5), 5);
    assert_eq!(get_hits_addend(100), 100);
}

// Example test showing how the system would work with actual Redis
// This would require testcontainers or a running Redis instance
/*
#[tokio::test]
async fn test_redis_rate_limiting() {
    // Start Redis container
    let docker = testcontainers::clients::Cli::default();
    let redis_container = docker.run(testcontainers::images::redis::Redis::default());
    let redis_port = redis_container.get_host_port_ipv4(6379);

    let redis_config = RedisConfig {
        url: format!("redis://localhost:{}", redis_port),
        ..Default::default()
    };

    let redis_pool = RedisClientPool::new_single(redis_config).await.unwrap();
    let cache = RedisRateLimitCache::new(redis_pool, 1000, 0.8, "test".to_string());
    let mut limiter = RateLimiter::new(Box::new(cache));

    // Add configuration
    let config = RateLimitConfig {
        domain: "test".to_string(),
        descriptors: vec![
            ConfigDescriptor {
                key: "api".to_string(),
                value: Some("endpoint".to_string()),
                rate_limit: Some(RateLimit {
                    requests_per_unit: 2,
                    unit: RateLimitUnit::Second,
                    unlimited: None,
                    name: None,
                }),
                shadow_mode: None,
                descriptors: None,
            },
        ],
    };
    
    let compiled_config = CompiledRateLimitConfig::compile(config).unwrap();
    limiter.add_config(compiled_config);

    // Test requests
    let request = RateLimitRequest {
        domain: "test".to_string(),
        descriptors: vec![
            RateLimitDescriptor {
                entries: vec![("api".to_string(), "endpoint".to_string())],
            }
        ],
        hits_addend: 1,
    };

    // First request should be allowed
    let response = limiter.should_rate_limit(&request).await.unwrap();
    assert_eq!(response.overall_code, ResponseCode::Ok);

    // Second request should be allowed
    let response = limiter.should_rate_limit(&request).await.unwrap();
    assert_eq!(response.overall_code, ResponseCode::Ok);

    // Third request should be over limit
    let response = limiter.should_rate_limit(&request).await.unwrap();
    assert_eq!(response.overall_code, ResponseCode::OverLimit);

    // Wait for window to reset
    sleep(Duration::from_secs(2)).await;

    // Request should be allowed again
    let response = limiter.should_rate_limit(&request).await.unwrap();
    assert_eq!(response.overall_code, ResponseCode::Ok);
}
*/