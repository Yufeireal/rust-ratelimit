#!/bin/bash

# Exit on any error
set -e

# Configuration
ECR_REPOSITORY="xxx"
IMAGE_NAME="fortio"
IMAGE_TAG="ratelimit-test"
REGION="us-west-2"  # Change this to your AWS region

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Logging functions
log_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

log_success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1"
}

log_warning() {
    echo -e "${YELLOW}[WARNING]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Check prerequisites
check_prerequisites() {
    log_info "Checking prerequisites..."
    
    # Check if Docker is running
    if ! docker info > /dev/null 2>&1; then
        log_error "Docker is not running. Please start Docker and try again."
        exit 1
    fi
    
    # Check if Docker Buildx is available
    if ! docker buildx version > /dev/null 2>&1; then
        log_error "Docker Buildx is not available. Please install Docker Buildx."
        exit 1
    fi
    
    # Check if AWS CLI is installed
    if ! command -v aws &> /dev/null; then
        log_error "AWS CLI is not installed. Please install AWS CLI."
        exit 1
    fi
    
    # Check if AWS credentials are configured
    if ! aws sts get-caller-identity &> /dev/null; then
        log_error "AWS credentials are not configured. Please run 'aws configure' or set up credentials."
        exit 1
    fi
    
    log_success "All prerequisites are met!"
}

# Create and use a new builder instance for multi-arch builds
setup_builder() {
    log_info "Setting up Docker Buildx builder for multi-architecture builds..."
    
    # Create a new builder instance if it doesn't exist
    if ! docker buildx inspect multiarch-builder > /dev/null 2>&1; then
        log_info "Creating new builder instance: multiarch-builder"
        docker buildx create --name multiarch-builder --driver docker-container --use
    else
        log_info "Using existing builder instance: multiarch-builder"
        docker buildx use multiarch-builder
    fi
    
    # Bootstrap the builder
    log_info "Bootstrapping builder instance..."
    docker buildx inspect --bootstrap
    
    log_success "Builder setup complete!"
}

# Login to ECR
login_to_ecr() {
    log_info "Logging in to Amazon ECR..."
    
    # Get ECR login token
    aws ecr get-login-password --region $REGION | docker login --username AWS --password-stdin $ECR_REPOSITORY
    
    log_success "Successfully logged in to ECR!"
}

# Build and push multi-architecture image
build_and_push() {
    log_info "Building and pushing multi-architecture Docker image..."
    
    # Full ECR repository URI
    ECR_URI="${ECR_REPOSITORY}:${IMAGE_TAG}"
    
    log_info "Building for architectures: linux/amd64, linux/arm64"
    log_info "Image will be tagged as: $ECR_URI"
    
    # Build and push using Docker Buildx
    docker buildx build \
        --platform linux/amd64,linux/arm64 \
        --tag $ECR_URI \
        --push \
        --file Dockerfile \
        --cache-from type=registry,ref=$ECR_URI \
        --cache-to type=inline \
        .
    
    log_success "Multi-architecture image built and pushed successfully!"
}

# Verify the pushed image
verify_push() {
    log_info "Verifying pushed image..."
    
    # List the manifest to verify both architectures
    log_info "Image manifest:"
    docker buildx imagetools inspect $ECR_URI
    
    log_success "Image verification complete!"
}

# Cleanup function
cleanup() {
    log_info "Cleaning up..."
    
    # Remove any dangling images
    docker image prune -f
    
    log_success "Cleanup complete!"
}

# Main execution
main() {
    log_info "Starting Docker image build and push process..."
    log_info "ECR Repository: $ECR_REPOSITORY"
    log_info "Image Name: $IMAGE_NAME"
    log_info "Image Tag: $IMAGE_TAG"
    log_info "Region: $REGION"
    echo
    
    # Execute all steps
    check_prerequisites
    setup_builder
    login_to_ecr
    build_and_push
    verify_push
    cleanup
    
    echo
    log_success "Build and push process completed successfully!"
    log_info "Your multi-architecture image is now available in ECR:"
    log_info "  $ECR_URI"
}

# Handle script interruption
trap 'log_error "Build process interrupted. Cleaning up..."; cleanup; exit 1' INT TERM

# Run main function
main "$@"
