#!/bin/bash
set -e

cd "$(dirname "$0")"

ACTION="${1:-run}"

case "$ACTION" in
  build)
    echo "=== Building Navidrome Radio ==="
    echo ""

    # Stop any running processes
    echo "→ Stopping any running backend processes..."
    pkill -f "navidrome-radio" || true
    sleep 1

    # Build frontend
    echo ""
    echo "→ Building frontend..."
    cd frontend
    npm run build
    cd ..

    # Build backend (embeds frontend)
    echo ""
    echo "→ Building backend with embedded frontend..."
    cd backend
    cargo build --release
    cd ..

    echo ""
    echo "✓ Build complete!"
    echo "  Binary: backend/target/release/navidrome-radio"
    echo "  Size: $(du -h backend/target/release/navidrome-radio | cut -f1)"
    echo ""
    echo "Run with: ./dev.sh run"
    ;;

  clean)
    echo "=== Cleaning build artifacts ==="
    echo ""

    # Stop any running processes
    echo "→ Stopping any running backend processes..."
    pkill -f "navidrome-radio" || true
    sleep 1

    echo "→ Cleaning frontend..."
    cd frontend
    rm -rf build .svelte-kit node_modules/.vite
    cd ..

    echo "→ Cleaning backend..."
    cd backend
    cargo clean
    cd ..

    echo ""
    echo "✓ Clean complete!"
    echo ""
    echo "Rebuild with: ./dev.sh rebuild"
    ;;

  rebuild)
    echo "=== Full Rebuild ==="
    $0 clean
    echo ""
    $0 build
    ;;

  run)
    echo "=== Starting Navidrome Radio ==="
    echo ""

    # Check if binary exists
    if [ ! -f "backend/target/release/navidrome-radio" ]; then
      echo "Binary not found! Building first..."
      echo ""
      $0 build
      echo ""
    fi

    # Start docker services if not running
    echo "→ Ensuring Docker services are running..."
    docker-compose up -d postgres redis
    sleep 2

    # Load .env file if it exists
    if [ -f .env ]; then
      echo "→ Loading environment variables from .env"
      export $(grep -v '^#' .env | xargs)
    else
      echo "⚠️  Warning: .env file not found!"
      echo "   Create .env with your Navidrome credentials:"
      echo "   cp .env.example .env"
      echo ""
      exit 1
    fi

    echo ""
    echo "→ Starting backend server..."
    echo "   Database: postgresql://postgres:postgres@localhost:5432/navidrome_radio"
    echo "   Redis: redis://localhost:6379"
    echo "   Navidrome: ${NAVIDROME_URL}"
    echo "   Server: http://localhost:8000"
    echo ""
    echo "Press Ctrl+C to stop"
    echo ""

    cd backend

    export DATABASE_URL="postgresql://postgres:postgres@localhost:5432/navidrome_radio"
    export REDIS_URL="redis://localhost:6379"
    export SERVER_HOST="0.0.0.0"
    export SERVER_PORT="8000"
    export RUST_LOG="${RUST_LOG:-info}"

    ./target/release/navidrome-radio
    ;;

  *)
    echo "Navidrome Radio - Development Tool"
    echo ""
    echo "Usage: ./dev.sh [command]"
    echo ""
    echo "Commands:"
    echo "  run      - Run the application (default)"
    echo "  build    - Build frontend and backend"
    echo "  clean    - Clean build artifacts"
    echo "  rebuild  - Clean and build from scratch"
    echo ""
    echo "Examples:"
    echo "  ./dev.sh          # Run the app"
    echo "  ./dev.sh build    # Build everything"
    echo "  ./dev.sh rebuild  # Clean rebuild"
    ;;
esac
