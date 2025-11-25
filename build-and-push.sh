#!/bin/bash
set -e

echo "Building and pushing multiarch Docker images..."

# Check if docker buildx is available
if ! docker buildx version &> /dev/null; then
    echo "Error: docker buildx is not available"
    exit 1
fi

# Create builder if it doesn't exist
docker buildx create --name multiarch-builder --use 2>/dev/null || docker buildx use multiarch-builder

# Build and push backend
echo "Building backend image..."
cd backend
docker buildx build --platform linux/amd64,linux/arm64 \
    -t ethanbarclay/navidrome-radio-backend:latest \
    --push .
cd ..

# Build and push frontend
echo "Building frontend image..."
cd frontend
docker buildx build --platform linux/amd64,linux/arm64 \
    -t ethanbarclay/navidrome-radio-frontend:latest \
    --push .
cd ..

# Build and push unified image
echo "Building unified image (backend + frontend)..."
docker buildx build --platform linux/amd64,linux/arm64 \
    -f Dockerfile.unified \
    -t ethanbarclay/navidrome-radio:latest \
    --push .

echo "Done! Images pushed to Docker Hub:"
echo "  - ethanbarclay/navidrome-radio-backend:latest"
echo "  - ethanbarclay/navidrome-radio-frontend:latest"
echo "  - ethanbarclay/navidrome-radio:latest (unified - backend + frontend)"
