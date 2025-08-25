use redis::{aio::ConnectionManager, AsyncCommands, RedisResult};
use std::time::Duration;
use crate::error::{Result, RateLimitError};

/// Redis client configuration
#[derive(Debug, Clone)]
pub struct RedisConfig {
    pub url: String,
    pub pool_size: Option<usize>,
    pub connection_timeout: Option<Duration>,
    pub command_timeout: Option<Duration>,
    pub enable_pipelining: bool,
}

impl Default for RedisConfig {
    fn default() -> Self {
        Self {
            url: "redis://localhost:6379".to_string(),
            pool_size: Some(10),
            connection_timeout: Some(Duration::from_secs(5)),
            command_timeout: Some(Duration::from_secs(1)),
            enable_pipelining: true,
        }
    }
}

/// Redis client wrapper for rate limiting operations
#[derive(Clone)]
pub struct RedisClient {
    connection: ConnectionManager,
    config: RedisConfig,
}

impl RedisClient {
    /// Create a new Redis client
    pub async fn new(config: RedisConfig) -> Result<Self> {
        use tracing::{info, warn};
        
        info!("Creating Redis client for URL: {}", config.url);
        
        // Add timeout to client creation
        let client = redis::Client::open(config.url.clone())
            .map_err(|e| {
                warn!("Failed to create Redis client: {}", e);
                RateLimitError::Redis(e)
            })?;
        
        info!("Redis client created, establishing connection manager...");
        
        // Add timeout for connection manager creation
        let connection_result = tokio::time::timeout(
            config.connection_timeout.unwrap_or(Duration::from_secs(10)),
            client.get_connection_manager()
        ).await;
        
        let connection = match connection_result {
            Ok(Ok(conn)) => {
                info!("Connection manager established successfully");
                conn
            }
            Ok(Err(e)) => {
                warn!("Failed to create connection manager: {}", e);
                return Err(RateLimitError::Redis(e));
            }
            Err(_) => {
                warn!("Timeout while creating connection manager ({}s)", 
                      config.connection_timeout.unwrap_or(Duration::from_secs(10)).as_secs());
                return Err(RateLimitError::Service(
                    "Timeout while creating Redis connection manager".to_string()
                ));
            }
        };

        info!("Testing Redis connection with PING...");
        
        // Test the connection with timeout
        let mut conn = connection.clone();
        let ping_result = tokio::time::timeout(
            config.command_timeout.unwrap_or(Duration::from_secs(5)),
            redis::cmd("PING").query_async::<_, ()>(&mut conn)
        ).await;
        
        match ping_result {
            Ok(Ok(_)) => {
                info!("Redis PING successful");
            }
            Ok(Err(e)) => {
                warn!("Redis PING failed: {}", e);
                return Err(RateLimitError::Redis(e));
            }
            Err(_) => {
                warn!("Redis PING timeout ({}s)", 
                      config.command_timeout.unwrap_or(Duration::from_secs(5)).as_secs());
                return Err(RateLimitError::Service(
                    "Timeout while testing Redis connection".to_string()
                ));
            }
        }

        info!("Redis client initialized successfully");
        Ok(Self { connection, config })
    }

    /// Increment a key by the given amount and set expiration
    pub async fn increment_and_expire(
        &self,
        key: &str,
        increment: u64,
        expire_seconds: u64,
    ) -> Result<u64> {
        let mut conn = self.connection.clone();
        
        if self.config.enable_pipelining {
            let pipe = redis::pipe()
                .atomic()
                .incr(key, increment)
                .expire(key, expire_seconds as i64)
                .query_async(&mut conn)
                .await
                .map_err(RateLimitError::Redis)?;
            
            match pipe {
                redis::Value::Bulk(values) if !values.is_empty() => {
                    if let redis::Value::Int(count) = &values[0] {
                        Ok(*count as u64)
                    } else {
                        Err(RateLimitError::Redis(redis::RedisError::from((
                            redis::ErrorKind::TypeError,
                            "Expected integer response from INCR",
                        ))))
                    }
                }
                _ => Err(RateLimitError::Redis(redis::RedisError::from((
                    redis::ErrorKind::TypeError,
                    "Unexpected pipeline response",
                )))),
            }
        } else {
            // Execute commands sequentially if pipelining is disabled
            let count: u64 = conn.incr(key, increment).await.map_err(RateLimitError::Redis)?;
            let _: bool = conn.expire(key, expire_seconds as i64).await.map_err(RateLimitError::Redis)?;
            Ok(count)
        }
    }

    /// Get the current value of a key
    pub async fn get(&self, key: &str) -> Result<Option<u64>> {
        let mut conn = self.connection.clone();
        let result: RedisResult<u64> = conn.get(key).await;
        
        match result {
            Ok(value) => Ok(Some(value)),
            Err(e) => {
                // Check if this is a nil value error by looking at the error kind
                if e.kind() == redis::ErrorKind::TypeError {
                    Ok(None)
                } else {
                    Err(RateLimitError::Redis(e))
                }
            }
        }
    }

    /// Execute multiple increment and expire operations in a pipeline
    pub async fn pipeline_increment_and_expire(
        &self,
        operations: Vec<(String, u64, u64)>,
    ) -> Result<Vec<u64>> {
        if operations.is_empty() {
            return Ok(vec![]);
        }

        let mut conn = self.connection.clone();
        let mut pipe = redis::pipe();
        pipe.atomic();

        // Add all operations to the pipeline
        for (key, increment, expire_seconds) in &operations {
            pipe.incr(key, *increment).expire(key, *expire_seconds as i64);
        }

        let results: Vec<redis::Value> = pipe
            .query_async(&mut conn)
            .await
            .map_err(RateLimitError::Redis)?;

        // Extract increment results (every 2nd value is the INCR result)
        let mut counts = Vec::new();
        for i in (0..results.len()).step_by(2) {
            if let redis::Value::Int(count) = &results[i] {
                counts.push(*count as u64);
            } else {
                return Err(RateLimitError::Redis(redis::RedisError::from((
                    redis::ErrorKind::TypeError,
                    "Expected integer response from pipeline INCR",
                ))));
            }
        }

        Ok(counts)
    }

    /// Check if the connection is healthy
    pub async fn health_check(&self) -> Result<()> {
        let mut conn = self.connection.clone();
        redis::cmd("PING").query_async::<_, ()>(&mut conn).await.map_err(RateLimitError::Redis)?;
        Ok(())
    }
}

/// Redis client pool for managing multiple connections
#[derive(Clone)]
pub struct RedisClientPool {
    primary_client: RedisClient,
    per_second_client: Option<RedisClient>,
}

impl RedisClientPool {
    /// Create a new Redis client pool with primary client only
    pub async fn new_single(config: RedisConfig) -> Result<Self> {
        use tracing::{info, warn};
        
        info!("Creating single Redis client pool...");
        
        let primary_client = match RedisClient::new(config).await {
            Ok(client) => {
                info!("Primary Redis client created successfully");
                client
            }
            Err(e) => {
                warn!("Failed to create primary Redis client: {}", e);
                return Err(e);
            }
        };
        
        info!("Single Redis pool created successfully");
        Ok(Self {
            primary_client,
            per_second_client: None,
        })
    }

    /// Create a new Redis client pool with separate per-second client
    pub async fn new_dual(
        primary_config: RedisConfig,
        per_second_config: RedisConfig,
    ) -> Result<Self> {
        use tracing::{info, warn};
        
        info!("Creating dual Redis client pool...");
        
        info!("Creating primary Redis client...");
        let primary_client = match RedisClient::new(primary_config).await {
            Ok(client) => {
                info!("Primary Redis client created successfully");
                client
            }
            Err(e) => {
                warn!("Failed to create primary Redis client: {}", e);
                return Err(e);
            }
        };
        
        info!("Creating per-second Redis client...");
        let per_second_client = match RedisClient::new(per_second_config).await {
            Ok(client) => {
                info!("Per-second Redis client created successfully");
                Some(client)
            }
            Err(e) => {
                warn!("Failed to create per-second Redis client: {}", e);
                return Err(e);
            }
        };
        
        info!("Dual Redis pool created successfully");
        Ok(Self {
            primary_client,
            per_second_client,
        })
    }

    /// Get the appropriate client for the given operation
    pub fn get_client(&self, is_per_second: bool) -> &RedisClient {
        if is_per_second && self.per_second_client.is_some() {
            self.per_second_client.as_ref().unwrap()
        } else {
            &self.primary_client
        }
    }

    /// Health check all clients
    pub async fn health_check(&self) -> Result<()> {
        self.primary_client.health_check().await?;
        if let Some(per_second_client) = &self.per_second_client {
            per_second_client.health_check().await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    // Note: Testcontainers Redis test would require specific version and proper imports
    // For now, we'll test the logic without actual Redis
    // async fn setup_redis() -> TestContainer {
    //     // Would set up Redis container for integration testing
    // }

    #[tokio::test]
    async fn test_redis_config() {
        let config = RedisConfig::default();
        assert_eq!(config.url, "redis://localhost:6379");
        assert!(config.enable_pipelining);
        assert_eq!(config.pool_size, Some(10));
    }

    #[tokio::test]
    async fn test_redis_client_pool_creation() {
        // Test basic pool creation logic without actual Redis
        let config1 = RedisConfig {
            url: "redis://localhost:6379".to_string(),
            ..Default::default()
        };

        let config2 = RedisConfig {
            url: "redis://localhost:6380".to_string(),
            ..Default::default()
        };

        // These would fail without actual Redis, but we can test the structure
        assert_ne!(config1.url, config2.url);
    }
}