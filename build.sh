#!/bin/bash

# Exit on any error
set -e

# Configuration
AWS_ACCOUNT_ID="505834710180"
ECR_REPOSITORY="${AWS_ACCOUNT_ID}.dkr.ecr.us-west-2.amazonaws.com/verkada/rust-ratelimit"
IMAGE_NAME="rust-ratelimit"
IMAGE_TAG="latest"
REGION="us-west-2"

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

# Login to ECR
login_to_ecr() {
    log_info "Logging in to Amazon ECR..."
    
    # Get ECR login token
    aws ecr get-login-password --region $REGION | docker login --username AWS --password-stdin $ECR_REPOSITORY
    
    log_success "Successfully logged in to ECR!"
}

# Build and push AMD64 image only
build_and_push() {
    log_info "Building and pushing AMD64 Docker image..."
    
    # Full ECR repository URI
    ECR_URI="${ECR_REPOSITORY}:${IMAGE_TAG}"
    
    log_info "Building for architecture: linux/amd64 only"
    log_info "Image will be tagged as: $ECR_URI"
    
    # Simple Docker build (no buildx needed)
    docker build -t $ECR_URI .
    
    # Push the image
    docker push $ECR_URI
    
    log_success "AMD64 image built and pushed successfully!"
}

# Verify the pushed image
verify_push() {
    log_info "Verifying pushed image..."
    
    # Show image details
    ECR_URI="${ECR_REPOSITORY}:${IMAGE_TAG}"
    log_info "Image details:"
    docker manifest inspect $ECR_URI 2>/dev/null || aws ecr describe-images --repository-name verkada/rust-ratelimit --region $REGION
    
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
    log_info "Architecture: AMD64 only"
    echo
    
    # Execute all steps
    check_prerequisites
    login_to_ecr
    build_and_push
    verify_push
    cleanup
    
    echo
    log_success "Build and push process completed successfully!"
    log_info "Your AMD64 image is now available in ECR:"
    log_info "  ${ECR_REPOSITORY}:${IMAGE_TAG}"
}

# Handle script interruption
trap 'log_error "Build process interrupted. Cleaning up..."; cleanup; exit 1' INT TERM

# Run main function
main "$@"