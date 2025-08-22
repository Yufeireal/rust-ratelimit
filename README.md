# Rust Rate Limit Service

A Rust implementation of the Envoy rate limit service with Redis backend, translated from the [original Go implementation](https://github.com/envoyproxy/ratelimit).

## Features

- **Redis Backend**: Uses Redis for distributed rate limiting with support for:
  - Single Redis instance
  - Dual Redis setup (separate per-second and other time units)
  - Connection pooling and pipelining
  - TLS support (configurable)

- **Flexible Configuration**: YAML-based configuration supporting:
  - Domain-based rate limits
  - Nested descriptors
  - Multiple time units (second, minute, hour, day)
  - Shadow mode for testing
  - Unlimited rate limits

- **Performance Optimizations**:
  - Local LRU cache for over-limit keys
  - Redis pipelining for batch operations
  - Efficient cache key generation with time windows

- **Monitoring & Observability**:
  - Prometheus metrics
  - Health checks
  - Structured logging with tracing
  - gRPC and HTTP interfaces

- **Production Ready**:
  - Async/await throughout
  - Error handling with proper types
  - Comprehensive test coverage
  - Docker support

## Architecture

The service is built with the following components:

1. **RateLimitService**: gRPC service interface (compatible with Envoy's rate limit filter)
2. **RateLimiter**: Core rate limiting logic and configuration management
3. **RedisRateLimitCache**: Redis-backed cache implementation
4. **RedisClientPool**: Connection management with support for dual Redis setup
5. **Configuration**: YAML-based configuration with compilation for fast lookups
6. **Metrics**: Prometheus metrics for monitoring and observability

## Rate Limiting Algorithm

The service implements a **fixed window counter** algorithm:

1. Requests are grouped by domain and descriptor combinations
2. Cache keys include time windows (e.g., `domain:key_value:timestamp_window`)
3. Redis `INCR` + `EXPIRE` operations track request counts
4. Local cache stores over-limit keys to avoid repeated Redis queries
5. Supports shadow mode for testing without enforcement

## Quick Start

### Prerequisites

- Rust 1.70+
- Redis server

### Configuration

Create a configuration file (see `config/example.yaml`):

```yaml
domain: my_service
descriptors:
  - key: api
    value: public
    rate_limit:
      requests_per_unit: 100
      unit: minute
```

### Environment Variables

```bash
# Redis configuration
REDIS_URL=redis://localhost:6379
REDIS_PERSECOND_URL=redis://localhost:6380  # Optional: separate per-second Redis

# Cache configuration  
LOCAL_CACHE_SIZE=1000
NEAR_LIMIT_RATIO=0.8
CACHE_KEY_PREFIX=ratelimit

# Server configuration
HTTP_PORT=0.0.0.0:8080
GRPC_PORT=0.0.0.0:8081
CONFIG_PATH=config/example.yaml

# Logging
RUST_LOG=rust_ratelimit=debug
```

### Running

```bash
# Run with cargo
cargo run

# Or build and run
cargo build --release
./target/release/rust-ratelimit
```

### Docker

```bash
# Build
docker build -t rust-ratelimit .

# Run
docker run -p 8080:8080 -p 8081:8081 \
  -e REDIS_URL=redis://redis:6379 \
  -v $(pwd)/config:/config \
  rust-ratelimit
```

## API

### gRPC Interface

The service implements the Envoy RateLimitService interface:

```proto
service RateLimitService {
  rpc ShouldRateLimit(RateLimitRequest) returns (RateLimitResponse);
}
```

### HTTP Endpoints

- `GET /healthcheck` - Health check
- `GET /metrics` - Prometheus metrics

## Configuration Format

The service uses YAML configuration files compatible with the original Go implementation:

```yaml
domain: <domain_name>
descriptors:
  - key: <descriptor_key>
    value: <descriptor_value>  # optional
    rate_limit:
      requests_per_unit: <number>
      unit: <second|minute|hour|day>
      unlimited: <boolean>      # optional
    shadow_mode: <boolean>      # optional
    descriptors:               # optional nested descriptors
      - key: <nested_key>
        # ... nested configuration
```

### Examples

#### Simple Rate Limit
```yaml
domain: api
descriptors:
  - key: endpoint
    value: search
    rate_limit:
      requests_per_unit: 100
      unit: minute
```

#### Nested Descriptors
```yaml
domain: messaging
descriptors:
  - key: message_type
    value: marketing
    descriptors:
      - key: to_number
        rate_limit:
          requests_per_unit: 5
          unit: day
```

#### Shadow Mode (Testing)
```yaml
domain: test
descriptors:
  - key: user
    value: test_user
    rate_limit:
      requests_per_unit: 10
      unit: second
    shadow_mode: true  # Always returns OK but tracks metrics
```

## Metrics

The service exposes Prometheus metrics at `/metrics`:

- `ratelimit_total_requests` - Total rate limit requests
- `ratelimit_over_limit_requests` - Requests that exceeded limits
- `ratelimit_within_limit_requests` - Requests within limits  
- `ratelimit_shadow_mode_requests` - Shadow mode overrides
- `ratelimit_local_cache_hits/misses` - Local cache performance
- `ratelimit_redis_operations` - Redis operation counts
- `ratelimit_redis_operation_duration_seconds` - Redis latency
- `ratelimit_config_load_success/error` - Configuration loading

## Development

### Running Tests

```bash
# Unit tests
cargo test

# Integration tests (requires Redis)
cargo test --test integration_tests

# With test containers (if available)
cargo test --features testcontainers
```

### Code Structure

```
src/
├── lib.rs          # Public API
├── main.rs         # Application entry point
├── cache.rs        # Rate limit cache trait and Redis implementation
├── config.rs       # Configuration parsing and compilation
├── error.rs        # Error types
├── limiter.rs      # Core rate limiting logic
├── metrics.rs      # Prometheus metrics
├── redis.rs        # Redis client and connection management
├── service.rs      # gRPC service implementation
└── utils.rs        # Utilities (time, cache keys, etc.)
```

## Differences from Go Implementation

While maintaining API compatibility, this Rust implementation includes some improvements:

1. **Type Safety**: Strong typing throughout with proper error handling
2. **Async/Await**: Native async support instead of goroutines
3. **Memory Safety**: Rust's ownership system prevents common bugs
4. **Performance**: Optimized data structures and algorithms
5. **Configuration**: Compile-time optimized config lookups

## Contributing

1. Fork the repository
2. Create a feature branch
3. Add tests for new functionality
4. Ensure all tests pass: `cargo test`
5. Run linting: `cargo clippy`
6. Format code: `cargo fmt`
7. Submit a pull request

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.

## Acknowledgments

This implementation is based on the [Envoy rate limit service](https://github.com/envoyproxy/ratelimit) originally written in Go. Thanks to the Envoy community for the excellent design and documentation.# rust-ratelimit
