# Multi-platform Docker build using separate stages per architecture
FROM --platform=linux/amd64 rust:1.82-slim as amd64-builder
ARG TARGETPLATFORM
ARG BUILDPLATFORM

# Install build dependencies for AMD64
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    protobuf-compiler \
    curl \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY build.rs ./
COPY proto/ ./proto/
COPY src/ ./src/

# Build for x86_64
RUN cargo build --release

FROM --platform=linux/arm64 rust:1.82-slim as arm64-builder
ARG TARGETPLATFORM
ARG BUILDPLATFORM

# Install build dependencies for ARM64
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    protobuf-compiler \
    curl \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY build.rs ./
COPY proto/ ./proto/
COPY src/ ./src/

# Build for aarch64
RUN cargo build --release

# Final stage - choose the right binary
FROM --platform=$TARGETPLATFORM debian:bookworm-slim as runtime

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user
RUN useradd -r -s /bin/false appuser

WORKDIR /app

# Copy the binary from the correct builder stage based on target platform
# This uses Docker's conditional copy feature
ARG TARGETPLATFORM
COPY --from=amd64-builder --chmod=755 /app/target/release/rust-ratelimit /tmp/rust-ratelimit-amd64
COPY --from=arm64-builder --chmod=755 /app/target/release/rust-ratelimit /tmp/rust-ratelimit-arm64

# Move the correct binary based on target platform
RUN if [ "$TARGETPLATFORM" = "linux/amd64" ]; then \
        mv /tmp/rust-ratelimit-amd64 /app/rust-ratelimit && rm -f /tmp/rust-ratelimit-arm64; \
    elif [ "$TARGETPLATFORM" = "linux/arm64" ]; then \
        mv /tmp/rust-ratelimit-arm64 /app/rust-ratelimit && rm -f /tmp/rust-ratelimit-amd64; \
    else \
        echo "Unsupported platform: $TARGETPLATFORM" && exit 1; \
    fi

# Copy configuration files if needed
COPY config/ ./config/

# Change ownership to non-root user
RUN chown -R appuser:appuser /app

# Switch to non-root user
USER appuser

# Expose the port your service runs on
EXPOSE 50051

# Health check
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:50051/health || exit 1

# Run the application
CMD ["./rust-ratelimit"]