use anyhow::Result;
use axum::{extract::State, http::StatusCode, response::Json, routing::get, Router};
use prometheus::TextEncoder;
use serde_json::json;
use std::{net::SocketAddr, sync::Arc};
use tokio::{net::TcpListener, signal};
use tonic::transport::Server;
use tracing::{info, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use rust_ratelimit::{
    cache::RedisRateLimitCache,
    config::{load_config_from_file, CompiledRateLimitConfig},
    error::RateLimitError,
    limiter::RateLimiter,
    metrics::Metrics,
    proto::{RateLimitServiceServer, RateLimitRequest, RateLimitResponse},
    redis::{RedisClientPool, RedisConfig},
    service::RateLimitService,
};

#[derive(Clone)]
struct AppState {
    service: Arc<RateLimitService>,
    metrics: Arc<Metrics>,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "rust_ratelimit=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    info!("Starting Rust Rate Limit Service");

    // Initialize components
    let metrics = Arc::new(Metrics::new()?);
    let service = create_service(metrics.clone()).await?;
    let state = AppState { service, metrics };

    // Load initial configuration if provided
    if let Ok(config_path) = std::env::var("CONFIG_PATH") {
        load_and_add_config(&state, &config_path).await?;
    }

    // Start HTTP server for health checks and metrics
    let http_addr = std::env::var("HTTP_PORT")
        .unwrap_or_else(|_| "0.0.0.0:8080".to_string())
        .parse::<SocketAddr>()?;
    
    let http_server = start_http_server(state.clone(), http_addr);

    // Start gRPC server
    let grpc_addr = std::env::var("GRPC_PORT")
        .unwrap_or_else(|_| "0.0.0.0:8081".to_string())
        .parse::<SocketAddr>()?;
    
    let grpc_server = start_grpc_server(state.service.clone(), grpc_addr);

    info!("HTTP server listening on {}", http_addr);
    info!("gRPC server listening on {}", grpc_addr);

    // Wait for shutdown signal
    tokio::select! {
        result = http_server => {
            if let Err(e) = result {
                warn!("HTTP server error: {}", e);
            }
        }
        result = grpc_server => {
            if let Err(e) = result {
                warn!("gRPC server error: {}", e);
            }
        }
        _ = signal::ctrl_c() => {
            info!("Received Ctrl+C, shutting down");
        }
    }

    info!("Service stopped");
    Ok(())
}

async fn create_service(metrics: Arc<Metrics>) -> Result<Arc<RateLimitService>> {
    // Configure Redis
    let redis_url = std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://localhost:6379".to_string());
    let redis_config: RedisConfig = RedisConfig {
        url: redis_url,
        ..Default::default()
    };

    // Check if per-second Redis is configured
    let redis_pool = if let Ok(per_second_url) = std::env::var("REDIS_PERSECOND_URL") {
        let per_second_config = RedisConfig {
            url: per_second_url,
            ..Default::default()
        };
        RedisClientPool::new_dual(redis_config, per_second_config).await?
    } else {
        RedisClientPool::new_single(redis_config).await?
    };

    // Create cache
    let local_cache_size = std::env::var("LOCAL_CACHE_SIZE")
        .unwrap_or_else(|_| "1000".to_string())
        .parse::<usize>()
        .unwrap_or(1000);

    let near_limit_ratio = std::env::var("NEAR_LIMIT_RATIO")
        .unwrap_or_else(|_| "0.8".to_string())
        .parse::<f32>()
        .unwrap_or(0.8);

    let cache_key_prefix = std::env::var("CACHE_KEY_PREFIX").unwrap_or_default();

    let cache = RedisRateLimitCache::new(
        redis_pool,
        local_cache_size,
        near_limit_ratio,
        cache_key_prefix,
    );

    // Create limiter and service
    let limiter = RateLimiter::new(Box::new(cache));
    let service = Arc::new(RateLimitService::new(limiter, metrics));

    Ok(service)
}

async fn load_and_add_config(state: &AppState, config_path: &str) -> Result<()> {
    info!("Loading configuration from: {}", config_path);
    
    let config = load_config_from_file(config_path)?;
    let compiled_config = CompiledRateLimitConfig::compile(config)?;
    
    state.service.add_config(compiled_config).await?;
    
    info!("Configuration loaded successfully");
    Ok(())
}

async fn start_http_server(state: AppState, addr: SocketAddr) -> Result<()> {
    let app: Router = Router::new()
        .route("/healthcheck", get(health_check))
        .route("/metrics", get(metrics_handler))
        .with_state(state);

    let listener = TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    
    Ok(())
}

async fn start_grpc_server(service: Arc<RateLimitService>, addr: SocketAddr) -> Result<()> {
    info!("Starting gRPC server with tonic at {}", addr);
    
    // Create the gRPC service implementation using the generated protobuf types
    let grpc_service = RateLimitServiceImpl {
        rate_limit_service: service,
    };
    
    // Start the real tonic gRPC server with generated protobuf support
    Server::builder()
        .add_service(RateLimitServiceServer::new(grpc_service))
        .serve(addr)
        .await
        .map_err(|e| anyhow::anyhow!("gRPC server error: {}", e))?;
    
    Ok(())
}

// Production gRPC service implementation using generated protobuf types
#[derive(Clone)]
pub struct RateLimitServiceImpl {
    rate_limit_service: Arc<RateLimitService>,
}

#[tonic::async_trait]
impl rust_ratelimit::proto::RateLimitService for RateLimitServiceImpl {
    async fn should_rate_limit(
        &self,
        request: tonic::Request<RateLimitRequest>,
    ) -> Result<tonic::Response<RateLimitResponse>, tonic::Status> {
        let req = request.into_inner();
        
        // Convert protobuf request to internal request format
        let internal_request = rust_ratelimit::service::GrpcRateLimitRequest {
            domain: req.domain,
            descriptors: req.descriptors.into_iter().map(|desc| {
                rust_ratelimit::service::GrpcRateLimitDescriptor {
                    entries: desc.entries.into_iter().map(|entry| {
                        rust_ratelimit::service::GrpcRateLimitDescriptorEntry {
                            key: entry.key,
                            value: entry.value,
                        }
                    }).collect(),
                }
            }).collect(),
            hits_addend: req.hits_addend,
        };
        
        // Call our rate limit service
        match self.rate_limit_service.should_rate_limit_direct(internal_request).await {
            Ok(response) => {
                // Convert internal response to protobuf response
                let grpc_response = RateLimitResponse {
                    overall_code: match response.overall_code {
                        rust_ratelimit::cache::ResponseCode::Ok => 
                            rust_ratelimit::proto::ResponseCode::Ok as i32,
                        rust_ratelimit::cache::ResponseCode::OverLimit => 
                            rust_ratelimit::proto::ResponseCode::OverLimit as i32,
                    },
                    statuses: response.statuses.into_iter().map(|status| {
                        rust_ratelimit::proto::DescriptorStatus {
                            code: match status.code {
                                rust_ratelimit::cache::ResponseCode::Ok => 
                                    rust_ratelimit::proto::ResponseCode::Ok as i32,
                                rust_ratelimit::cache::ResponseCode::OverLimit => 
                                    rust_ratelimit::proto::ResponseCode::OverLimit as i32,
                            },
                            current_limit: status.current_limit.map(|limit| {
                                rust_ratelimit::proto::RateLimit {
                                    requests_per_unit: limit.requests_per_unit,
                                    unit: match limit.unit {
                                        rust_ratelimit::utils::Unit::Second => 
                                            rust_ratelimit::proto::rate_limit_response::rate_limit::Unit::Second as i32,
                                        rust_ratelimit::utils::Unit::Minute => 
                                            rust_ratelimit::proto::rate_limit_response::rate_limit::Unit::Minute as i32,
                                        rust_ratelimit::utils::Unit::Hour => 
                                            rust_ratelimit::proto::rate_limit_response::rate_limit::Unit::Hour as i32,
                                        rust_ratelimit::utils::Unit::Day => 
                                            rust_ratelimit::proto::rate_limit_response::rate_limit::Unit::Day as i32,
                                    },
                                }
                            }),
                            limit_remaining: status.limit_remaining,
                            duration_until_reset_secs: status.duration_until_reset_secs,
                        }
                    }).collect(),
                    response_headers_to_add: vec![],
                    request_headers_to_add: vec![],
                };
                
                Ok(tonic::Response::new(grpc_response))
            }
            Err(e) => {
                let status = match e {
                    RateLimitError::DomainNotFound(domain) => {
                        tonic::Status::not_found(format!("Domain not found: {}", domain))
                    }
                    RateLimitError::Service(msg) => {
                        tonic::Status::invalid_argument(format!("Service error: {}", msg))
                    }
                    RateLimitError::Redis(e) => {
                        tonic::Status::unavailable(format!("Redis error: {}", e))
                    }
                    _ => tonic::Status::internal(format!("Internal error: {}", e)),
                };
                Err(status)
            }
        }
    }
}

async fn health_check(State(state): State<AppState>) -> Result<Json<serde_json::Value>, StatusCode> {
    match state.service.health_check().await {
        Ok(()) => Ok(Json(json!({
            "status": "healthy",
            "timestamp": chrono::Utc::now().to_rfc3339()
        }))),
        Err(_) => Err(StatusCode::SERVICE_UNAVAILABLE),
    }
}

async fn metrics_handler(State(state): State<AppState>) -> Result<String, StatusCode> {
    let encoder = TextEncoder::new();
    let metric_families = state.metrics.registry().gather();
    
    match encoder.encode_to_string(&metric_families) {
        Ok(metrics) => Ok(metrics),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}