# Multi-stage build for SvelteKit frontend
FROM node:20-alpine as builder

WORKDIR /app

# Copy package files
COPY package.json package-lock.json ./

# Install dependencies
RUN npm ci

# Copy source code
COPY . .

# Build the app
RUN npm run build

# Runtime stage with Node adapter
FROM node:20-alpine

WORKDIR /app

# Copy built app from builder
COPY --from=builder /app/build ./build
COPY --from=builder /app/package.json ./
COPY --from=builder /app/node_modules ./node_modules

# Expose port
EXPOSE 3000

# Set environment variables
ENV NODE_ENV=production
ENV ORIGIN=http://localhost:3000

# Run the app
CMD ["node", "build"]
