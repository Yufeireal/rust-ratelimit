// Debug version to identify where the application hangs during startup
// Run this with: cargo run --bin debug_startup

use anyhow::Result;
use std::time::{Duration, Instant};
use tracing::{info, warn, error};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use rust_ratelimit::{
    redis::{RedisClientPool, RedisConfig},
};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing with more verbose output
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    info!("🚀 Starting debug startup test...");
    
    let start_time = Instant::now();
    
    // Test each component step by step with timeouts
    test_redis_connection().await?;
    
    info!("✅ All tests completed successfully in {:?}", start_time.elapsed());
    Ok(())
}

async fn test_redis_connection() -> Result<()> {
    info!("🔍 Testing Redis connection...");
    
    let redis_url = std::env::var("REDIS_URL")
        .unwrap_or_else(|_| "redis://localhost:6379".to_string());
    info!("Using Redis URL: {}", redis_url);
    
    let redis_config = RedisConfig {
        url: redis_url,
        connection_timeout: Some(Duration::from_secs(5)),
        command_timeout: Some(Duration::from_secs(3)),
        ..Default::default()
    };
    
    // Test with timeout
    let connection_start = Instant::now();
    info!("Creating Redis client with 10 second timeout...");
    
    let result = tokio::time::timeout(
        Duration::from_secs(10),
        RedisClientPool::new_single(redis_config)
    ).await;
    
    match result {
        Ok(Ok(pool)) => {
            info!("✅ Redis pool created successfully in {:?}", connection_start.elapsed());
            
            // Test health check
            info!("Testing health check...");
            let health_start = Instant::now();
            match pool.health_check().await {
                Ok(()) => {
                    info!("✅ Health check passed in {:?}", health_start.elapsed());
                }
                Err(e) => {
                    error!("❌ Health check failed: {}", e);
                    return Err(e.into());
                }
            }
        }
        Ok(Err(e)) => {
            error!("❌ Redis pool creation failed: {}", e);
            return Err(e.into());
        }
        Err(_) => {
            error!("❌ Redis pool creation timed out after 10 seconds");
            error!("This suggests a network connectivity issue or Redis server is not responding");
            return Err(anyhow::anyhow!("Redis connection timeout"));
        }
    }
    
    // Test per-second Redis if configured
    if let Ok(per_second_url) = std::env::var("REDIS_PERSECOND_URL") {
        info!("Testing per-second Redis: {}", per_second_url);
        
        let per_second_config = RedisConfig {
            url: per_second_url,
            connection_timeout: Some(Duration::from_secs(5)),
            command_timeout: Some(Duration::from_secs(3)),
            ..Default::default()
        };
        
        let per_second_start = Instant::now();
        let result = tokio::time::timeout(
            Duration::from_secs(10),
            RedisClientPool::new_single(per_second_config)
        ).await;
        
        match result {
            Ok(Ok(pool)) => {
                info!("✅ Per-second Redis pool created successfully in {:?}", per_second_start.elapsed());
                pool.health_check().await?;
                info!("✅ Per-second Redis health check passed");
            }
            Ok(Err(e)) => {
                error!("❌ Per-second Redis pool creation failed: {}", e);
                return Err(e.into());
            }
            Err(_) => {
                error!("❌ Per-second Redis pool creation timed out");
                return Err(anyhow::anyhow!("Per-second Redis connection timeout"));
            }
        }
    }
    
    Ok(())
}
