# Multi-stage build for single-binary Navidrome Radio
# Stage 1: Build frontend
FROM node:20-slim as frontend-builder

WORKDIR /app/frontend

# Copy frontend package files
COPY frontend/package.json frontend/package-lock.json ./

# Install dependencies
RUN npm ci

# Copy frontend source
COPY frontend/ ./

# Build frontend (static site)
RUN npm run build

# Stage 2: Build backend with embedded frontend
FROM rust:1.83-slim as backend-builder

WORKDIR /app

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Copy backend manifests
COPY backend/Cargo.toml backend/Cargo.lock ./

# Copy backend source
COPY backend/src ./src
COPY backend/migrations ./migrations

# Copy built frontend into backend's expected location
COPY --from=frontend-builder /app/frontend/build ../frontend/build

# Build backend for release (frontend will be embedded)
RUN cargo build --release

# Stage 3: Runtime
FROM debian:bookworm-slim

WORKDIR /app

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

# Copy binary from builder (frontend is embedded in it)
COPY --from=backend-builder /app/target/release/navidrome-radio /usr/local/bin/navidrome-radio

# Copy migrations
COPY --from=backend-builder /app/migrations /app/migrations

# Expose port
EXPOSE 8000

# Set environment variables
ENV RUST_LOG=info

# Run the binary
CMD ["navidrome-radio"]
