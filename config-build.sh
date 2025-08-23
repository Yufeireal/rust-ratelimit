#!/bin/bash

# Configuration script for Docker build
# Run this script to customize your build settings

echo "🐳 Docker Build Configuration for Rust Rate Limiter"
echo "=================================================="
echo

# Get current values
CURRENT_ECR=$(grep 'ECR_REPOSITORY=' build.sh | cut -d'"' -f2)
CURRENT_REGION=$(grep 'REGION=' build.sh | cut -d'"' -f2)
CURRENT_TAG=$(grep 'IMAGE_TAG=' build.sh | cut -d'"' -f2)

echo "Current configuration:"
echo "  ECR Repository: $CURRENT_ECR"
echo "  AWS Region: $CURRENT_REGION"
echo "  Image Tag: $CURRENT_TAG"
echo

# Get new values from user
read -p "Enter ECR Repository URI (e.g., 123456789012.dkr.ecr.us-east-1.amazonaws.com/my-repo): " NEW_ECR
read -p "Enter AWS Region (e.g., us-east-1): " NEW_REGION
read -p "Enter Image Tag (e.g., latest, v1.0.0): " NEW_TAG

# Update build.sh if values were provided
if [ ! -z "$NEW_ECR" ]; then
    sed -i "s|ECR_REPOSITORY=\"$CURRENT_ECR\"|ECR_REPOSITORY=\"$NEW_ECR\"|g" build.sh
    echo "✅ Updated ECR Repository to: $NEW_ECR"
fi

if [ ! -z "$NEW_REGION" ]; then
    sed -i "s|REGION=\"$CURRENT_REGION\"|REGION=\"$NEW_REGION\"|g" build.sh
    echo "✅ Updated AWS Region to: $NEW_REGION"
fi

if [ ! -z "$NEW_TAG" ]; then
    sed -i "s|IMAGE_TAG=\"$CURRENT_TAG\"|IMAGE_TAG=\"$NEW_TAG\"|g" build.sh
    echo "✅ Updated Image Tag to: $NEW_TAG"
fi

echo
echo "🎯 Configuration updated! You can now run:"
echo "  ./build.sh"
echo
echo "📝 To manually edit configuration, open build.sh and modify:"
echo "  - ECR_REPOSITORY: Your ECR repository URI"
echo "  - REGION: Your AWS region"
echo "  - IMAGE_TAG: Desired image tag"
