#!/bin/bash
set -e

echo "Testing single binary locally with docker-compose..."

# Make sure services are running
echo "→ Starting PostgreSQL, Redis, and Navidrome..."
docker-compose up -d postgres redis navidrome

# Wait for services to be ready
echo "→ Waiting for services to be ready..."
sleep 5

# Build the frontend
echo "→ Building frontend..."
cd frontend
npm install
npm run build
cd ..

# Build the backend with embedded frontend
echo "→ Building backend with embedded frontend..."
cd backend
cargo build --release
cd ..

echo ""
echo "✓ Build complete!"
echo ""
echo "→ Starting Navidrome Radio..."
echo "   Database: postgresql://postgres:postgres@localhost:5432/navidrome_radio"
echo "   Redis: redis://localhost:6379"
echo "   Navidrome: http://localhost:4533"
echo "   Server: http://localhost:8000"
echo ""

# Set environment variables and run
cd backend
export DATABASE_URL="postgresql://postgres:postgres@localhost:5432/navidrome_radio"
export REDIS_URL="redis://localhost:6379"
export NAVIDROME_URL="${NAVIDROME_URL:-http://10.1.1.3:4533}"
# export NAVIDROME_USER="${NAVIDROME_USER:-admin}"
# export NAVIDROME_PASSWORD="${NAVIDROME_PASSWORD:-admin}"
export JWT_SECRET="${JWT_SECRET:-dev-secret-change-in-production}"
export SERVER_HOST="0.0.0.0"
export SERVER_PORT="8000"
export RUST_LOG="info"

echo "Press Ctrl+C to stop the server"
echo ""

./target/release/navidrome-radio
