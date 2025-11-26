#!/bin/bash
set -e

cd "$(dirname "$0")"

echo "=== Building and Pushing Docker Image ==="
echo ""
echo "This builds a unified multiarch image (amd64 + arm64)"
echo "Image: ethanbarclay/navidrome-radio:latest"
echo ""

# Check if docker buildx is available
if ! docker buildx version &> /dev/null; then
    echo "Error: docker buildx is not available"
    echo "Install it with: docker buildx install"
    exit 1
fi

# Create builder if it doesn't exist
echo "→ Setting up buildx..."
docker buildx create --name multiarch-builder --use 2>/dev/null || docker buildx use multiarch-builder

# Build and push
echo ""
echo "→ Building for linux/amd64 and linux/arm64..."
docker buildx build \
    --platform linux/amd64,linux/arm64 \
    -f Dockerfile \
    -t ethanbarclay/navidrome-radio:latest \
    --push .

echo ""
echo "✓ Done! Image pushed to Docker Hub:"
echo "  ethanbarclay/navidrome-radio:latest (amd64, arm64)"
echo ""
echo "Run with:"
echo "  docker run -p 8000:8000 \\"
echo "    -e DATABASE_URL=postgresql://... \\"
echo "    -e REDIS_URL=redis://... \\"
echo "    -e NAVIDROME_URL=http://... \\"
echo "    -e NAVIDROME_USER=... \\"
echo "    -e NAVIDROME_PASSWORD=... \\"
echo "    -e JWT_SECRET=... \\"
echo "    ethanbarclay/navidrome-radio:latest"
echo ""
