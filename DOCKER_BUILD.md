# Docker Build Guide for Rust Rate Limiter

This guide explains how to build and push multi-architecture Docker images to Amazon ECR.

## 🚀 Quick Start

### 1. Configure Your Build
```bash
./config-build.sh
```
This interactive script will help you set:
- ECR Repository URI
- AWS Region
- Image Tag

### 2. Build and Push
```bash
./build.sh
```

## 📋 Prerequisites

Before running the build script, ensure you have:

- **Docker** running with Buildx support
- **AWS CLI** installed and configured
- **AWS credentials** with ECR permissions
- **ECR repository** created in your AWS account

### Install Docker Buildx
```bash
# Docker Buildx comes with Docker Desktop
# For Linux, you might need to install it separately
docker buildx version
```

### Configure AWS CLI
```bash
aws configure
# Enter your AWS Access Key ID, Secret Access Key, Region, and output format
```

### Verify AWS Credentials
```bash
aws sts get-caller-identity
```

## 🔧 Configuration

### Manual Configuration
Edit `build.sh` and modify these variables:

```bash
ECR_REPOSITORY="your-ecr-repository-uri"
IMAGE_NAME="rust-ratelimit"
IMAGE_TAG="latest"
REGION="us-east-1"
```

### ECR Repository Format
Your ECR repository should look like:
```
123456789012.dkr.ecr.us-east-1.amazonaws.com/my-repo
```

## 🏗️ Build Process

The build script performs these steps:

1. **Prerequisites Check**
   - Docker running
   - Docker Buildx available
   - AWS CLI installed
   - AWS credentials configured

2. **Builder Setup**
   - Creates multi-architecture builder instance
   - Bootstraps the builder

3. **ECR Authentication**
   - Logs in to Amazon ECR
   - Handles authentication tokens

4. **Multi-Architecture Build**
   - Builds for `linux/amd64` and `linux/arm64`
   - Uses Docker Buildx for cross-compilation
   - Pushes directly to ECR

5. **Verification**
   - Inspects pushed image manifest
   - Confirms both architectures available

6. **Cleanup**
   - Removes dangling images
   - Frees disk space

## 🐳 Dockerfile Features

### Multi-Stage Build
- **Builder stage**: Compiles Rust code with all dependencies
- **Runtime stage**: Minimal image with just the binary and runtime dependencies

### Multi-Architecture Support
- Uses `--platform=$BUILDPLATFORM` for proper cross-compilation
- Supports both Intel/AMD (amd64) and ARM (arm64) architectures

### Security
- Runs as non-root user (`appuser`)
- Minimal runtime dependencies
- Health checks included

### Optimization
- Dependency caching for faster builds
- Layer optimization
- Small final image size

## 📊 Build Output

When successful, you'll see:
```
[SUCCESS] Build and push process completed successfully!
[INFO] Your multi-architecture image is now available in ECR:
[INFO]   your-ecr-repository:latest
```

## 🔍 Troubleshooting

### Common Issues

#### Docker Buildx Not Available
```bash
# Install Docker Buildx
docker buildx install
```

#### AWS Credentials Error
```bash
# Configure AWS credentials
aws configure
# Or set environment variables
export AWS_ACCESS_KEY_ID=your_key
export AWS_SECRET_ACCESS_KEY=your_secret
export AWS_DEFAULT_REGION=your_region
```

#### ECR Login Failed
```bash
# Check ECR repository exists
aws ecr describe-repositories --region your-region

# Verify repository URI format
# Should be: account.dkr.ecr.region.amazonaws.com/repo-name
```

#### Build Fails on ARM64
```bash
# Ensure QEMU is available for ARM emulation
docker run --rm --privileged multiarch/qemu-user-static --reset -p yes
```

### Debug Mode
To see more detailed output, modify `build.sh`:
```bash
# Add this line after the shebang
set -x
```

## 🚀 Advanced Usage

### Custom Build Arguments
Modify the Dockerfile to accept build arguments:
```dockerfile
ARG RUST_VERSION=1.75
FROM --platform=$BUILDPLATFORM rust:${RUST_VERSION}-slim as builder
```

### Multiple Tags
To push multiple tags, modify the build command:
```bash
docker buildx build \
    --platform linux/amd64,linux/arm64 \
    --tag $ECR_REPOSITORY:latest \
    --tag $ECR_REPOSITORY:v1.0.0 \
    --push \
    --file Dockerfile \
    .
```

### Build Cache
The script includes build caching for faster subsequent builds:
```bash
--cache-from type=registry,ref=$ECR_URI \
--cache-to type=inline \
```

## 📝 File Structure

```
rust-ratelimit/
├── build.sh              # Main build script
├── config-build.sh       # Configuration helper
├── Dockerfile            # Multi-arch Dockerfile
├── .dockerignore         # Docker build exclusions
├── DOCKER_BUILD.md       # This guide
└── ...                   # Your Rust source code
```

## 🔗 Related Commands

### Check Image Manifest
```bash
docker buildx imagetools inspect your-ecr-repository:latest
```

### Pull and Run Locally
```bash
# Pull the image
docker pull your-ecr-repository:latest

# Run locally
docker run -p 50051:50051 your-ecr-repository:latest
```

### Clean Up Build Cache
```bash
docker buildx prune
docker system prune
```

## 📚 Additional Resources

- [Docker Buildx Documentation](https://docs.docker.com/buildx/)
- [Amazon ECR User Guide](https://docs.aws.amazon.com/ecr/)
- [Multi-Architecture Docker Images](https://docs.docker.com/buildx/working-with-buildx/)
- [Rust Docker Best Practices](https://github.com/rust-lang/docker-rust)

---

**Happy Building! 🐳✨**
