# Multi-stage build for single-binary Navidrome Radio
# Stage 1: Build frontend
FROM node:20-slim AS frontend-builder

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
FROM rustlang/rust:nightly-slim AS backend-builder

WORKDIR /app

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    g++ \
    && rm -rf /var/lib/apt/lists/*

# Copy backend manifests
COPY backend/Cargo.toml backend/Cargo.lock ./

# Copy backend source
COPY backend/src ./src
COPY backend/migrations ./migrations

# Copy sqlx offline query cache for offline compilation
COPY backend/.sqlx ./.sqlx

# Copy built frontend into backend's expected location
COPY --from=frontend-builder /app/frontend/build ../frontend/build

# Build backend for release (frontend will be embedded)
# SQLX_OFFLINE enables using cached queries without a live database
ENV SQLX_OFFLINE=true
RUN cargo build --release

# Stage 3: Runtime
FROM debian:bookworm-slim

WORKDIR /app

# ONNX Runtime version
ARG ORT_VERSION=1.19.2
ARG TARGETARCH

# Install runtime dependencies and ONNX Runtime
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    libstdc++6 \
    curl \
    && rm -rf /var/lib/apt/lists/* \
    # Determine ONNX Runtime architecture
    && if [ "$TARGETARCH" = "arm64" ]; then ORT_ARCH="aarch64"; else ORT_ARCH="x64"; fi \
    # Download and install ONNX Runtime
    && curl -fSL -o /tmp/onnxruntime.tgz \
       "https://github.com/microsoft/onnxruntime/releases/download/v${ORT_VERSION}/onnxruntime-linux-${ORT_ARCH}-${ORT_VERSION}.tgz" \
    && tar -xzf /tmp/onnxruntime.tgz -C /tmp \
    && cp /tmp/onnxruntime-linux-${ORT_ARCH}-${ORT_VERSION}/lib/libonnxruntime.so.${ORT_VERSION} /usr/lib/ \
    && ln -s /usr/lib/libonnxruntime.so.${ORT_VERSION} /usr/lib/libonnxruntime.so \
    && rm -rf /tmp/onnxruntime* \
    && ldconfig

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
