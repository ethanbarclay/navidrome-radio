#!/bin/bash
set -e

echo "Building Navidrome Radio as a single binary..."

# Step 1: Build frontend
echo "→ Building frontend..."
cd frontend
npm install
npm run build
cd ..

# Step 2: Build backend (embeds frontend)
echo "→ Building backend with embedded frontend..."
cd backend
cargo build --release
cd ..

echo "✓ Build complete!"
echo ""
echo "Binary location: backend/target/release/navidrome-radio"
echo "Binary size: $(du -h backend/target/release/navidrome-radio | cut -f1)"
echo ""
echo "To run locally:"
echo "  cd backend && ./target/release/navidrome-radio"
