#!/bin/bash
set -e

echo "Building multiarch Docker images..."

# Build backend for both architectures
echo "Building backend for linux/amd64..."
cd backend
docker build --platform linux/amd64 -t ethanbarclay/navidrome-radio-backend:latest-amd64 .
echo "Building backend for linux/arm64..."
docker build --platform linux/arm64 -t ethanbarclay/navidrome-radio-backend:latest-arm64 .
cd ..

# Build frontend for both architectures
echo "Building frontend for linux/amd64..."
cd frontend
docker build --platform linux/amd64 -t ethanbarclay/navidrome-radio-frontend:latest-amd64 .
echo "Building frontend for linux/arm64..."
docker build --platform linux/arm64 -t ethanbarclay/navidrome-radio-frontend:latest-arm64 .
cd ..

# Push all images
echo "Pushing images..."
docker push ethanbarclay/navidrome-radio-backend:latest-amd64
docker push ethanbarclay/navidrome-radio-backend:latest-arm64
docker push ethanbarclay/navidrome-radio-frontend:latest-amd64
docker push ethanbarclay/navidrome-radio-frontend:latest-arm64

# Create and push manifests
echo "Creating multi-arch manifests..."
docker manifest create ethanbarclay/navidrome-radio-backend:latest \
  ethanbarclay/navidrome-radio-backend:latest-amd64 \
  ethanbarclay/navidrome-radio-backend:latest-arm64

docker manifest create ethanbarclay/navidrome-radio-frontend:latest \
  ethanbarclay/navidrome-radio-frontend:latest-amd64 \
  ethanbarclay/navidrome-radio-frontend:latest-arm64

docker manifest push ethanbarclay/navidrome-radio-backend:latest
docker manifest push ethanbarclay/navidrome-radio-frontend:latest

echo "Done! Multi-arch images available:"
echo "  - ethanbarclay/navidrome-radio-backend:latest (amd64, arm64)"
echo "  - ethanbarclay/navidrome-radio-frontend:latest (amd64, arm64)"
