#!/bin/bash
set -e

echo "Building and pushing ARM64 unified Docker image..."

# Build for ARM64 only (native architecture for Apple Silicon)
echo "Building unified image for linux/arm64..."
docker build --platform linux/arm64 \
    -f Dockerfile \
    -t ethanbarclay/navidrome-radio:latest \
    -t ethanbarclay/navidrome-radio:arm64 \
    . && docker push ethanbarclay/navidrome-radio:latest && docker push ethanbarclay/navidrome-radio:arm64

echo ""
echo "✓ Done! ARM64 image pushed to Docker Hub:"
echo "  - ethanbarclay/navidrome-radio:latest (arm64 only)"
echo "  - ethanbarclay/navidrome-radio:arm64"
echo ""
echo "Note: This image only supports ARM64 architecture (Apple Silicon, Raspberry Pi, etc.)."
echo "For AMD64 support, you'll need to build on an AMD64 machine or use a CI/CD pipeline."
echo ""
echo "Required environment variables:"
echo "  - DATABASE_URL=postgresql://user:pass@host:5432/db"
echo "  - REDIS_URL=redis://host:6379"
echo "  - NAVIDROME_URL=http://host:4533"
echo "  - NAVIDROME_USER=your_username"
echo "  - NAVIDROME_PASSWORD=your_password"
echo "  - JWT_SECRET=your-secret-key"
echo ""
echo "Example usage:"
echo "  docker run -p 8000:8000 \\"
echo "    -e DATABASE_URL=postgresql://postgres:postgres@db:5432/navidrome_radio \\"
echo "    -e REDIS_URL=redis://redis:6379 \\"
echo "    -e NAVIDROME_URL=http://navidrome:4533 \\"
echo "    -e NAVIDROME_USER=admin \\"
echo "    -e NAVIDROME_PASSWORD=yourpassword \\"
echo "    -e JWT_SECRET=your-secret-key \\"
echo "    ethanbarclay/navidrome-radio:latest"
