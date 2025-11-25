# Navidrome Radio - Docker Deployment Guide

## Quick Start with Docker Compose

1. **Clone or download this repository**

2. **Create your `.env` file**:
```bash
cp .env.example .env
```

3. **Edit `.env` and configure**:
   - `NAVIDROME_USER` - Your Navidrome username
   - `NAVIDROME_PASSWORD` - Your Navidrome password
   - `JWT_SECRET` - A random secure string for JWT tokens
   - `ANTHROPIC_API_KEY` - (Optional) Your Anthropic API key for AI features
   - `MUSIC_DIR` - Path to your music library

4. **Start all services**:
```bash
docker-compose up -d
```

5. **Access the application**:
   - Frontend: http://localhost:3000
   - Backend API: http://localhost:8000
   - Navidrome: http://localhost:4533

## Services

The stack includes:
- **PostgreSQL** - Database (port 5432)
- **Redis** - Cache and session store (port 6379)
- **Navidrome** - Music server (port 4533)
- **Backend** - Rust API server (port 8000)
- **Frontend** - SvelteKit web UI (port 3000)

## Architecture Support

The images support:
- **linux/amd64** (x86_64) - Intel/AMD processors
- **linux/arm64** (aarch64) - ARM processors (Apple Silicon, Raspberry Pi 4+)

## AI Features

To enable AI-powered station creation:

1. Get an API key from https://console.anthropic.com/
2. Add it to your `.env`:
   ```
   ANTHROPIC_API_KEY=sk-ant-xxxxx
   ```
3. Restart the backend:
   ```bash
   docker-compose restart backend
   ```

The AI will analyze descriptions and find matching tracks in your library!

## Stopping the Stack

```bash
docker-compose down
```

To remove volumes (data will be lost):
```bash
docker-compose down -v
```

## Updating

Pull the latest images:
```bash
docker-compose pull
docker-compose up -d
```

## Troubleshooting

### Check logs
```bash
docker-compose logs -f backend
docker-compose logs -f frontend
```

### Database issues
```bash
# Reset database
docker-compose down
docker volume rm navidrome-radio_postgres_data
docker-compose up -d
```

### Port conflicts
If ports are already in use, edit `docker-compose.yml` and change the port mappings.

## Building from Source

If you want to build the images yourself instead of pulling from Docker Hub:

### With Buildx (multiarch)
```bash
./build-and-push.sh
```

### Without Buildx (current architecture only)
```bash
./build-and-push-simple.sh
```
