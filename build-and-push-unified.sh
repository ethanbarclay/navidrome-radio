#!/bin/bash
set -e

echo "Building and pushing unified multiarch Docker image..."

# Check if docker buildx is available
if ! docker buildx version &> /dev/null; then
    echo "Error: docker buildx is not available"
    exit 1
fi

# Create builder if it doesn't exist
docker buildx create --name multiarch-builder --use 2>/dev/null || docker buildx use multiarch-builder

# Build and push unified image (single binary with embedded frontend)
echo "Building unified image for linux/amd64 and linux/arm64..."
docker buildx build --platform linux/amd64,linux/arm64 \
    -f Dockerfile \
    -t ethanbarclay/navidrome-radio:latest \
    --push .

echo ""
echo "✓ Done! Unified image pushed to Docker Hub:"
echo "  - ethanbarclay/navidrome-radio:latest (amd64, arm64)"
echo ""
echo "This single image contains both frontend and backend in one binary!"
echo ""
echo "Required environment variables:"
echo "  - DATABASE_URL=postgresql://user:pass@host:5432/db"
echo "  - REDIS_URL=redis://host:6379"
echo "  - NAVIDROME_URL=http://host:4533"
echo "  - NAVIDROME_USER=your_username"
echo "  - NAVIDROME_PASSWORD=your_password"
echo "  - JWT_SECRET=your-secret-key"
echo ""
echo "Optional environment variables:"
echo "  - SERVER_HOST=0.0.0.0 (default)"
echo "  - SERVER_PORT=8000 (default)"
echo "  - RUST_LOG=info (default)"
echo "  - ANTHROPIC_API_KEY=sk-ant-... (for AI features)"
echo ""
echo "Example usage:"
echo "  docker run -p 8000:8000 \\"
echo "    -e DATABASE_URL=postgresql://postgres:postgres@db:5432/navidrome_radio \\"
echo "    -e REDIS_URL=redis://redis:6379 \\"
echo "    -e NAVIDROME_URL=http://navidrome:4533 \\"
echo "    -e NAVIDROME_USER=admin \\"
echo "    -e NAVIDROME_PASSWORD=yourpassword \\"
echo "    -e JWT_SECRET=your-secret-key \\"
echo "    ethanbarclay/navidrome-radio:latest
