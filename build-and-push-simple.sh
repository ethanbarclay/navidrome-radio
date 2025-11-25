#!/bin/bash
set -e

echo "Building and pushing Docker images for current architecture..."

# Build and push backend
echo "Building backend image..."
cd backend
docker build -t ethanbarclay/navidrome-radio-backend:latest .
docker push ethanbarclay/navidrome-radio-backend:latest
cd ..

# Build and push frontend
echo "Building frontend image..."
cd frontend
docker build -t ethanbarclay/navidrome-radio-frontend:latest .
docker push ethanbarclay/navidrome-radio-frontend:latest
cd ..

echo "Done! Images pushed to Docker Hub:"
echo "  - ethanbarclay/navidrome-radio-backend:latest"
echo "  - ethanbarclay/navidrome-radio-frontend:latest"
echo ""
echo "Note: These are single-architecture images. For multiarch support,"
echo "install Docker buildx and use build-and-push.sh instead."
