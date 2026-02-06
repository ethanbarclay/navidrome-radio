# Navidrome Radio

Turn your Navidrome music library into synchronized radio stations. All listeners hear the same track at the same time, like a real radio station.

Built with Rust and SvelteKit.

## Features

- **Multiple Radio Stations** - Create unlimited stations from your music library
- **Synchronized Playback** - All listeners hear the same track at the same position
- **AI-Powered Curation** - Claude selects tracks based on natural language descriptions (optional)
- **Audio Similarity** - ML-based audio embeddings find sonically similar tracks (optional)
- **Hybrid Curation** - LLM picks seed songs, ML fills gaps with similar tracks
- **Mobile-First Design** - Responsive UI with system media controls and animated visualizer
- **Admin Dashboard** - Create stations, manage library, view listener counts
- **Custom Branding** - Set your own site title in settings
- **Real-time Updates** - Live now playing info and listener counts

## Quick Start

### Prerequisites

- Docker and Docker Compose
- A running Navidrome instance on your network
- Navidrome admin credentials

### 1. Create Configuration

```bash
mkdir navidrome-radio && cd navidrome-radio

cat > .env << 'EOF'
# Required - Your Navidrome server
NAVIDROME_URL=http://192.168.1.100:4533
NAVIDROME_USER=admin
NAVIDROME_PASSWORD=your-navidrome-password

# Required - Generate with: openssl rand -base64 32
JWT_SECRET=your-secure-random-secret-at-least-32-chars

# Optional - Enable AI track selection
ANTHROPIC_API_KEY=sk-ant-...
EOF
```

### 2. Create docker-compose.yml

```yaml
version: '3.9'

services:
  postgres:
    image: pgvector/pgvector:pg16
    restart: unless-stopped
    environment:
      POSTGRES_DB: navidrome_radio
      POSTGRES_USER: postgres
      POSTGRES_PASSWORD: postgres
    volumes:
      - postgres_data:/var/lib/postgresql/data
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U postgres"]
      interval: 10s
      timeout: 5s
      retries: 5

  redis:
    image: redis:7-alpine
    restart: unless-stopped
    command: redis-server --maxmemory 256mb --maxmemory-policy allkeys-lru
    volumes:
      - redis_data:/data
    healthcheck:
      test: ["CMD", "redis-cli", "ping"]
      interval: 10s
      timeout: 3s
      retries: 5

  navidrome-radio:
    image: ethanbarclay/navidrome-radio:latest
    restart: unless-stopped
    ports:
      - "8000:8000"
    environment:
      DATABASE_URL: postgresql://postgres:postgres@postgres:5432/navidrome_radio
      REDIS_URL: redis://redis:6379
      NAVIDROME_URL: ${NAVIDROME_URL}
      NAVIDROME_USER: ${NAVIDROME_USER}
      NAVIDROME_PASSWORD: ${NAVIDROME_PASSWORD}
      JWT_SECRET: ${JWT_SECRET}
      ANTHROPIC_API_KEY: ${ANTHROPIC_API_KEY:-}
      SERVER_HOST: 0.0.0.0
      SERVER_PORT: 8000
    depends_on:
      postgres:
        condition: service_healthy
      redis:
        condition: service_healthy

volumes:
  postgres_data:
  redis_data:
```

### 3. Start

```bash
docker-compose up -d
```

### 4. Create Your First Station

1. Open http://localhost:8000
2. Register an account (first user becomes admin)
3. Go to Admin Dashboard
4. Click "Sync Library" to import tracks from Navidrome
5. Click "[3] CREATE" tab
6. Enter station name, path, and description
7. (Optional) Click "AI: FIND TRACKS" to let AI curate a playlist
8. Click "Create Station" then "Start"

Listen at `http://localhost:8000/station/your-station-path`

## Configuration

### Environment Variables

| Variable | Required | Description |
|----------|----------|-------------|
| `NAVIDROME_URL` | Yes | Your Navidrome server URL |
| `NAVIDROME_USER` | Yes | Navidrome username |
| `NAVIDROME_PASSWORD` | Yes | Navidrome password |
| `JWT_SECRET` | Yes | Random string, min 32 chars |
| `ANTHROPIC_API_KEY` | No | Enables AI track curation |
| `NAVIDROME_LIBRARY_PATH` | No | Path to music files for audio embeddings |
| `CORS_ORIGINS` | No | Allowed origins (default: localhost) |
| `SERVER_PORT` | No | Server port (default: 8000) |

### Audio Embeddings (Optional)

For ML-based audio similarity, Navidrome Radio needs access to your music files:

```yaml
navidrome-radio:
  # ... other config ...
  environment:
    NAVIDROME_LIBRARY_PATH: /music
  volumes:
    - /path/to/your/music:/music:ro
```

After starting, go to Admin > Library and click "Generate Embeddings". The ONNX model (~160MB) downloads automatically on first use.

### Reverse Proxy

For production, put behind a reverse proxy with HTTPS. Example Caddy config:

```
radio.example.com {
    reverse_proxy localhost:8000
}
```

Set `CORS_ORIGINS=https://radio.example.com` in your .env file.

## How It Works

### Synchronized Playback

1. Backend tracks current song and start time for each station
2. Clients poll `/nowplaying` endpoint every 10 seconds
3. Audio player syncs to the correct position
4. When a track ends, backend auto-selects the next track

### Track Selection Modes

- **Manual** - Admin picks specific tracks when creating station
- **Random** - Random tracks matching station genres
- **AI Contextual** - Claude analyzes description and selects tracks (requires API key)
- **Hybrid** - LLM picks 5-10 seed songs, ML finds similar tracks to fill gaps

### Hybrid Curation Flow

1. You describe the vibe: "relaxing acoustic music for a rainy day"
2. LLM analyzes your library and picks perfect seed songs
3. You can regenerate any seed you don't like
4. ML audio encoder finds sonically similar tracks between seeds
5. Result: a playlist that matches your description AND flows smoothly

## Admin Features

### Stations Tab
- Start/stop broadcasting
- View listener counts
- See station track lists
- Export to Navidrome playlist

### Library Tab
- Sync tracks from Navidrome
- Generate audio embeddings
- View embedding visualization (2D plot of your library by audio similarity)

### Create Tab
- Natural language station description
- AI-assisted track curation with seed review
- Genre tagging

### Settings Tab
- Custom site title (replaces "NAVIDROME RADIO" on homepage)

## API Reference

### Authentication
- `POST /api/v1/auth/register` - Create account
- `POST /api/v1/auth/login` - Get JWT token
- `GET /api/v1/auth/me` - Current user info

### Stations
- `GET /api/v1/stations` - List stations
- `POST /api/v1/stations` - Create station (admin)
- `GET /api/v1/stations/:id/nowplaying` - Now playing info
- `POST /api/v1/stations/:id/start` - Start broadcast (admin)
- `POST /api/v1/stations/:id/stop` - Stop broadcast (admin)
- `POST /api/v1/stations/:id/skip` - Skip track (admin)

### Settings
- `GET /api/v1/settings` - Get app settings
- `PUT /api/v1/settings` - Update settings (admin)

### Streaming
- `GET /api/v1/navidrome/stream/:track_id` - Audio stream (proxied)
- `GET /api/v1/navidrome/cover/:track_id` - Album art (proxied)

## Development

### Prerequisites

- Rust 1.75+
- Node.js 20+
- Docker (for PostgreSQL and Redis)

### Setup

```bash
git clone https://github.com/ethanbarclay/navidrome-radio.git
cd navidrome-radio

# Start infrastructure
docker-compose up -d postgres redis

# Copy and edit environment
cp .env.example .env

# Build and run
./dev.sh run
```

### Project Structure

```
navidrome-radio/
├── frontend/           # SvelteKit app
│   ├── src/
│   │   ├── routes/     # Pages (/, /admin, /station/[path])
│   │   └── lib/        # Components, API client, stores
│   └── build/          # Static output (embedded in backend)
├── backend/            # Rust server
│   ├── src/
│   │   ├── api/        # HTTP endpoints
│   │   ├── services/   # Business logic
│   │   └── models/     # Data types
│   └── migrations/     # PostgreSQL schemas
├── docker-compose.yml
├── Dockerfile
└── dev.sh              # Development helper
```

### Build Commands

```bash
./dev.sh run      # Build and run locally
./dev.sh build    # Build only
./dev.sh rebuild  # Clean rebuild
./dev.sh clean    # Remove build artifacts
```

## Tech Stack

**Frontend:** SvelteKit 5, TypeScript, Tailwind CSS 4, Three.js (visualizer), Plotly.js (embeddings)

**Backend:** Rust, Axum, SQLx, ONNX Runtime (audio ML)

**Infrastructure:** PostgreSQL 16 + pgvector, Redis 7

## Troubleshooting

### No audio plays
- Check Navidrome is accessible from Docker network
- Verify NAVIDROME_URL, USER, PASSWORD in .env
- Ensure Navidrome has finished scanning your library

### Station won't start
- Sync library first (Admin > Library > Sync Library)
- Check station has matching genres or AI-curated tracks

### AI features not working
- Verify ANTHROPIC_API_KEY is set
- Check logs: `docker-compose logs navidrome-radio`

### Embeddings fail to generate
- Mount music library with NAVIDROME_LIBRARY_PATH
- Ensure read access to music files
- Check disk space for ONNX model (~160MB)

### Frontend shows stale content
- The backend embeds frontend at build time
- Rebuild: `./dev.sh rebuild`
- Hard refresh browser: Cmd+Shift+R / Ctrl+Shift+R

## License

MIT License

## Acknowledgments

- [Navidrome](https://www.navidrome.org/) - Open-source music server
- [Axum](https://github.com/tokio-rs/axum) - Rust web framework
- [SvelteKit](https://kit.svelte.dev/) - Frontend framework
