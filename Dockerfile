# Build stage
FROM rust:1.75 as builder

WORKDIR /usr/src/app

# Copy manifests
COPY Cargo.toml Cargo.lock ./

# Copy source code
COPY src ./src
COPY tests ./tests

# Build the application
RUN cargo build --release

# Runtime stage
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && \
    apt-get install -y ca-certificates && \
    rm -rf /var/lib/apt/lists/*

# Create application user
RUN useradd -m -u 1000 ratelimit

# Copy the binary from builder stage
COPY --from=builder /usr/src/app/target/release/rust-ratelimit /usr/local/bin/rust-ratelimit

# Create config directory
RUN mkdir -p /config && chown ratelimit:ratelimit /config

# Copy example configuration
COPY config/example.yaml /config/

# Switch to non-root user
USER ratelimit

# Expose ports
EXPOSE 8080 8081

# Environment variables with defaults
ENV HTTP_PORT=0.0.0.0:8080
ENV GRPC_PORT=0.0.0.0:8081
ENV REDIS_URL=redis://redis:6379
ENV CONFIG_PATH=/config/example.yaml
ENV RUST_LOG=rust_ratelimit=info

# Health check
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:8080/healthcheck || exit 1

# Run the application
CMD ["rust-ratelimit"]