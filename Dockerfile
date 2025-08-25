# AMD64 only Rust Docker build
FROM rust:1.82-slim as builder

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    protobuf-compiler \
    curl \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy source files
COPY Cargo.toml Cargo.lock ./
COPY build.rs ./
COPY proto/ ./proto/
COPY src/ ./src/

# Build for AMD64
RUN cargo build --release

# Runtime stage
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy binary and config
COPY --from=builder /app/target/release/rust-ratelimit ./rust-ratelimit

# Make binary executable
RUN chmod +x ./rust-ratelimit

# Expose the ports your service actually uses
EXPOSE 8000 8001

# Run the application
CMD ["./rust-ratelimit"]
