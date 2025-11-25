#!/bin/bash
set -e

echo "Building and pushing AMD64-only unified Docker image..."

# Build for AMD64 only (single architecture)
echo "Building unified image for linux/amd64..."
docker build --platform linux/amd64 \
    -f Dockerfile \
    -t ethanbarclay/navidrome-radio:latest \
    -t ethanbarclay/navidrome-radio:amd64 \
    --push .

echo ""
echo "✓ Done! AMD64 image pushed to Docker Hub:"
echo "  - ethanbarclay/navidrome-radio:latest (amd64 only)"
echo "  - ethanbarclay/navidrome-radio:amd64"
echo ""
echo "Note: This image only supports AMD64 architecture."
echo "For ARM64 support, you'll need to build multiarch after freeing disk space."
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
echo "    ethanbarclay/navidrome-radio:latest"
