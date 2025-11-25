# Multi-stage build for Rust backend
FROM rust:1.83-slim as builder

WORKDIR /app

# Install dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Copy manifests
COPY Cargo.toml Cargo.lock ./

# Copy source code
COPY src ./src
COPY migrations ./migrations

# Build for release
RUN cargo build --release

# Runtime stage
FROM debian:bookworm-slim

WORKDIR /app

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

# Copy the binary from builder
COPY --from=builder /app/target/release/navidrome-radio /usr/local/bin/navidrome-radio

# Copy migrations
COPY --from=builder /app/migrations /app/migrations

# Expose port
EXPOSE 8000

# Set environment variables
ENV RUST_LOG=info

# Run the binary
CMD ["navidrome-radio"]
