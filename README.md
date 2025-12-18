# Navidrome Radio

AI-powered radio station platform that transforms your Navidrome music library into synchronized streaming radio stations. Built with Rust and SvelteKit for maximum performance and a beautiful mobile-first experience.

## ✨ Features

- **📻 Multiple Radio Stations** - Create unlimited virtual radio stations from your music library
- **🎵 Synchronized Playback** - All listeners hear the same track at the same time
- **🤖 AI-Powered Curation** - Intelligent track selection based on station descriptions (optional)
- **🧠 ML Audio Similarity** - Audio embeddings for finding sonically similar tracks (optional)
- **🎯 Hybrid Curation** - LLM selects perfect seed songs, ML fills gaps with similar tracks
- **📊 Embedding Visualization** - Interactive 3D visualization of your music library
- **📱 Mobile-First Design** - Beautiful responsive UI with system media controls (Android/iOS/macOS)
- **👑 Admin Controls** - Create, start, stop stations and skip tracks
- **⚡ Real-time Updates** - Live listener counts and now playing information
- **🔐 Secure Authentication** - JWT-based auth with Argon2 password hashing
- **🎨 Modern UI** - Clean, dark theme with Tailwind CSS

## 🚀 Quick Start

### Development (Recommended)

```bash
# Clone the repository
git clone <your-repo-url>
cd navidrome-radio

# Start infrastructure (PostgreSQL, Redis, Navidrome)
docker-compose up -d postgres redis navidrome

# Build and run (one command!)
./dev.sh run
```

Access at **http://localhost:8000**

### Production (Docker)

```bash
# Start everything
docker-compose up -d

# Access the application
open http://localhost:8000
```

## 📋 Prerequisites

**For Development:**
- Docker and Docker Compose
- Rust 1.75+ (https://rustup.rs/)
- Node.js 20+ (https://nodejs.org/)

**For Production:**
- Docker and Docker Compose only

**Music Library:**
- Navidrome server with your music collection

## 🎯 First Steps

### 1. Configure Environment

```bash
cp .env.example .env
```

Edit `.env`:
```bash
# Required
NAVIDROME_URL=http://localhost:4533
NAVIDROME_USER=your_username
NAVIDROME_PASSWORD=your_password
JWT_SECRET=change-this-to-random-secure-string

# Optional - for AI features
ANTHROPIC_API_KEY=sk-ant-...
```

### 2. Set Up Navidrome

If you don't have Navidrome yet:
1. Start it: `docker-compose up -d navidrome`
2. Open http://localhost:4533
3. Create admin account
4. Add your music library path in Navidrome settings
5. Wait for library scan to complete

### 3. Create Your First Station

1. Navigate to http://localhost:8000
2. Register an account (first user becomes admin)
3. Go to Admin Dashboard
4. Click "Create New Station"
5. Fill in:
   - **Name**: "Chill Vibes"
   - **Path**: "chill-vibes"
   - **Description**: "Relaxing indie and acoustic music"
   - **Genres**: "Indie Rock, Acoustic, Folk"
6. Click "Start" to begin broadcasting

### 4. Listen

Navigate to `http://localhost:8000/station/chill-vibes` and click "Start Listening"!

## 🛠 Development

### Development Workflow

```bash
# First time setup or complete rebuild
./dev.sh rebuild

# Run the application
./dev.sh run

# Just rebuild (incremental)
./dev.sh build

# Clean build artifacts
./dev.sh clean
```

### What happens when you run?

1. **`./dev.sh run`** starts:
   - PostgreSQL (port 5432)
   - Redis (port 6379)
   - Navidrome (port 4533)
   - Navidrome Radio (port 8000)

2. Access the app at: **http://localhost:8000**

### Scripts Reference

#### `./dev.sh` - Main development tool
All-in-one script for local development.

**Commands:**
- `./dev.sh run` - Start the application (default)
- `./dev.sh build` - Build frontend + backend
- `./dev.sh clean` - Clean all build artifacts
- `./dev.sh rebuild` - Full clean rebuild

**What it does:**
- Builds frontend with npm
- Builds backend with cargo (embeds frontend)
- Manages Docker services
- Runs the application locally

#### `./docker-build.sh` - Build production image
Builds and pushes multi-architecture Docker image to Docker Hub.

```bash
./docker-build.sh
```

**What it does:**
- Builds for linux/amd64 and linux/arm64
- Pushes to `ethanbarclay/navidrome-radio:latest`
- Requires Docker buildx

### What Gets Built

- **Frontend**: SvelteKit app compiled to static files
- **Backend**: Rust binary with embedded frontend (single binary deployment!)

### Project Structure

```
navidrome-radio/
├── frontend/          # SvelteKit frontend
│   ├── src/
│   │   ├── routes/      # Pages
│   │   └── lib/         # Components
│   └── build/        # Built static files (embedded in backend)
├── backend/          # Rust backend
│   ├── src/
│   │   ├── api/         # HTTP endpoints
│   │   ├── services/    # Business logic
│   │   └── models/      # Data models
│   ├── migrations/      # Database schemas
│   └── target/
│       └── release/
│           └── navidrome-radio  # Single binary with embedded frontend
├── docker-compose.yml   # Full stack deployment
├── Dockerfile        # Unified image (frontend + backend)
├── dev.sh           # Development tool
└── docker-build.sh  # Production Docker build
```

## 🏗️ Architecture

```
┌─────────────────────────────────┐
│   Frontend (SvelteKit)          │
│   - Station List                │
│   - Player UI                   │
│   - Admin Dashboard             │
│   - Media Session API           │
└────────────┬────────────────────┘
             │ HTTP/REST
┌────────────▼────────────────────┐
│   Backend (Rust + Axum)         │
│   - JWT Auth                    │
│   - Station Manager             │
│   - Track Curation              │
│   - Streaming Proxy             │
└──┬────────┬────────┬────────────┘
   │        │        │
   │        │        └─────────┐
   │        │                  │
┌──▼────┐ ┌▼─────┐  ┌─────────▼───┐
│ Postgre│ │ Redis│  │  Navidrome  │
│   SQL  │ │      │  │   (Music)   │
└────────┘ └──────┘  └─────────────┘
```

## 🎨 Tech Stack

**Frontend:**
- SvelteKit 5 (Svelte Runes)
- TypeScript
- Tailwind CSS 4
- Media Session API
- Plotly.js for embedding visualization
- UMAP for dimensionality reduction

**Backend:**
- Rust with Axum framework
- SQLx for PostgreSQL
- Redis for caching
- JWT authentication
- Single binary with embedded frontend
- ONNX Runtime for ML inference (audio encoder)
- Symphonia for audio decoding

**Infrastructure:**
- PostgreSQL 16 with pgvector extension
- Redis 7
- Docker & Docker Compose

## 🔧 Configuration

### Environment Variables

| Variable | Description | Default | Required |
|----------|-------------|---------|----------|
| `DATABASE_URL` | PostgreSQL connection | `postgresql://...` | Yes |
| `REDIS_URL` | Redis connection | `redis://localhost:6379` | Yes |
| `NAVIDROME_URL` | Navidrome server URL | - | Yes |
| `NAVIDROME_USER` | Navidrome username | - | Yes |
| `NAVIDROME_PASSWORD` | Navidrome password | - | Yes |
| `JWT_SECRET` | JWT signing secret | - | Yes |
| `ANTHROPIC_API_KEY` | Claude API (optional) | - | No |
| `NAVIDROME_LIBRARY_PATH` | Path to music files for audio analysis | - | No |
| `AUDIO_ENCODER_MODEL_PATH` | Path to ONNX audio encoder model | - | No |
| `SERVER_HOST` | Bind address | `0.0.0.0` | No |
| `SERVER_PORT` | Server port | `8000` | No |
| `RUST_LOG` | Log level | `info` | No |

### Station Configuration

When creating a station, you can configure:

- **Genres**: Comma-separated list (e.g., "Rock, Alternative, Indie")
- **Description**: Rich description for AI curation
- **Track Selection**:
  - `random`: Random from genres (default, no API key needed)
  - `ai_contextual`: AI-powered based on description (requires API key)
  - `ai_embeddings`: Similarity-based (requires API key)
  - `hybrid`: Mix of AI and random

### ML Audio Features (Optional)

Navidrome Radio supports ML-powered audio similarity for creating sonically coherent playlists. This requires:

1. **ONNX Audio Encoder Model** - A pre-trained model that converts audio to 100-dimensional embeddings
2. **Access to Music Files** - Direct filesystem access to your Navidrome music library
3. **pgvector Extension** - PostgreSQL extension for vector similarity search (included in docker-compose)

#### How Hybrid Curation Works

1. **Seed Selection**: LLM analyzes your query (e.g., "relaxing acoustic music") and selects 5-10 perfect seed songs from your library
2. **Genre Awareness**: LLM automatically determines relevant genres (Jazz, Ambient, Folk) instead of just using keywords
3. **Gap Filling**: Audio encoder finds sonically similar tracks to place between seeds
4. **Result**: A playlist that matches your query AND flows smoothly from track to track

#### Setting Up Audio Embeddings

```bash
# 1. Set environment variables
export NAVIDROME_LIBRARY_PATH=/path/to/your/music
export AUDIO_ENCODER_MODEL_PATH=/path/to/audio_encoder.onnx

# 2. Use pgvector-enabled PostgreSQL (already in docker-compose.yml)
# Image: pgvector/pgvector:pg16

# 3. In Admin Dashboard, click "Sync Library" then "Generate Audio Embeddings"
# This analyzes your music files and stores embeddings for similarity search
```

#### Embedding Visualization

The Admin Dashboard includes an interactive 3D visualization of your music library based on audio embeddings. Tracks that sound similar appear closer together, letting you explore your library's sonic landscape.

## 📡 API Reference

### Authentication
- `POST /api/v1/auth/register` - Register new user
- `POST /api/v1/auth/login` - Login
- `GET /api/v1/auth/me` - Get current user

### Stations
- `GET /api/v1/stations` - List all stations
- `POST /api/v1/stations` - Create station (admin)
- `GET /api/v1/stations/:id` - Get station details
- `PATCH /api/v1/stations/:id` - Update station (admin)
- `DELETE /api/v1/stations/:id` - Delete station (admin)
- `POST /api/v1/stations/:id/start` - Start broadcasting (admin)
- `POST /api/v1/stations/:id/stop` - Stop broadcasting (admin)
- `POST /api/v1/stations/:id/skip` - Skip current track (admin)
- `GET /api/v1/stations/:id/nowplaying` - Get now playing info

### Streaming
- `GET /api/v1/navidrome/stream/:track_id` - Stream audio
- `GET /api/v1/navidrome/cover/:track_id` - Get album art

## 🐳 Docker Deployment

### Quick Deploy

```bash
# Build production image
./docker-build.sh

# Or use docker-compose
docker-compose up -d
```

### Manual Docker Run

```bash
docker run -p 8000:8000 \
  -e DATABASE_URL=postgresql://... \
  -e REDIS_URL=redis://... \
  -e NAVIDROME_URL=http://... \
  -e NAVIDROME_USER=admin \
  -e NAVIDROME_PASSWORD=password \
  -e JWT_SECRET=your-secret \
  ethanbarclay/navidrome-radio:latest
```

## 🔒 Production Checklist

Before deploying to production:

- [ ] Change `JWT_SECRET` to a strong random value (use `openssl rand -base64 32`)
- [ ] Use strong database passwords
- [ ] Set up HTTPS with reverse proxy (Caddy, nginx, Traefik)
- [ ] Configure firewall rules
- [ ] Set up backups for PostgreSQL
- [ ] Configure Redis maxmemory policy
- [ ] Set up monitoring and logging
- [ ] Review and restrict CORS settings if needed

## 🐛 Troubleshooting

### Backend won't start
```bash
# Check if services are running
docker-compose ps

# Check logs
docker-compose logs backend

# Verify environment variables
cat .env

# Check port availability
lsof -i :8000
```

### No audio plays
```bash
# Check Navidrome connection
curl http://localhost:4533/ping

# Verify credentials in .env
# Ensure Navidrome has scanned your music library
# Check that genres match your library
```

### Frontend shows old code after rebuild
The backend embeds the frontend at build time. After frontend changes:

```bash
# Full clean rebuild
./dev.sh rebuild

# Hard refresh in browser
# macOS: Cmd + Shift + R
# Windows/Linux: Ctrl + Shift + R
```

### Media controls don't show album art
```bash
# Check browser console for errors
# Verify cover URL returns 200:
curl -I http://localhost:8000/api/v1/navidrome/cover/TRACK_ID

# Try hard refresh in browser
```

### Port already in use
```bash
# Kill any running backend
pkill -f "navidrome-radio"

# Or restart everything
./dev.sh rebuild
```

### Database/Redis issues
```bash
# Restart Docker services
docker-compose restart postgres redis navidrome

# Or rebuild from scratch
docker-compose down -v
./dev.sh run
```

## 🗺️ Roadmap

- [ ] WebSocket support for real-time updates
- [ ] Playlist history and analytics
- [ ] User favorites and recommendations
- [ ] Scheduled programming
- [ ] Social features (chat, reactions)
- [ ] HLS streaming with live transcoding
- [ ] Mobile apps (React Native)
- [ ] Multi-server federation

## 🤝 Contributing

Contributions are welcome!

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/amazing-feature`)
3. Commit your changes (`git commit -m 'Add amazing feature'`)
4. Push to the branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request

## 📄 License

MIT License - see LICENSE file for details

## 🙏 Acknowledgments

- [Navidrome](https://www.navidrome.org/) - The amazing open-source music server
- [Axum](https://github.com/tokio-rs/axum) - Fast and ergonomic Rust web framework
- [SvelteKit](https://kit.svelte.dev/) - The fastest way to build web apps
- [Tailwind CSS](https://tailwindcss.com/) - Utility-first CSS framework

## 📐 Technical Specification

### System Architecture

Navidrome Radio is a distributed radio station platform with the following components:

**Core Services:**
- **Backend (Rust)**: Axum-based HTTP server with JWT authentication
- **Database (PostgreSQL)**: Persistent storage for users, stations, and metadata
- **Cache (Redis)**: Session management and track selection caching
- **Music Source (Navidrome)**: Subsonic API-compatible music server

### How It Works

#### Station Broadcasting

1. **Station Creation** (Admin):
   - Define station metadata (name, description, genres)
   - Select track selection mode (random, AI-contextual, AI-embeddings, hybrid)
   - Station stored in PostgreSQL

2. **Track Selection**:
   - **Random Mode**: Queries Navidrome for tracks matching genre filters
   - **AI Mode**: Uses Claude API to analyze track metadata and select contextually appropriate tracks
   - **Hybrid Mode**: Mixes AI-selected and random tracks
   - Selected tracks cached in Redis for performance

3. **Synchronized Playback**:
   - Backend tracks current track and start time for each station
   - All clients query `/nowplaying` endpoint (polling every 10s)
   - Clients calculate elapsed time and sync audio position
   - When track ends, backend automatically selects next track

4. **Audio Streaming**:
   - Backend acts as proxy to Navidrome's Subsonic API
   - Clients request audio via `/stream/:track_id`
   - Backend authenticates with Navidrome and forwards stream
   - Album art served via `/cover/:track_id`

#### Authentication Flow

1. User registers via `/api/v1/auth/register`
2. Password hashed with Argon2
3. Login returns JWT token with user ID and role
4. Subsequent requests include `Authorization: Bearer <token>`
5. Middleware validates JWT and extracts user info
6. Admin-only endpoints check `role = "admin"`

#### Frontend Architecture

**Pages:**
- `/` - Station list with live listener counts
- `/station/:path` - Station player with now playing info
- `/admin` - Admin dashboard for station management

**Key Features:**
- Media Session API integration (Android/iOS/macOS controls)
- Automatic position synchronization
- Graceful handling of browser autoplay policies
- Responsive mobile-first design

### Data Models

**User:**
```rust
struct User {
    id: Uuid,
    username: String,
    password_hash: String, // Argon2
    role: String,          // "user" or "admin"
    created_at: DateTime,
}
```

**Station:**
```rust
struct Station {
    id: Uuid,
    name: String,
    path: String,          // URL slug
    description: String,
    genres: Vec<String>,
    is_active: bool,
    selection_mode: String, // "random", "ai_contextual", etc.
    current_track_id: Option<String>,
    started_at: Option<DateTime>,
    created_at: DateTime,
}
```

**Track (from Navidrome):**
```typescript
interface Track {
    id: string;
    title: string;
    artist: string;
    album: string;
    duration: number; // seconds
    genre?: string;
}
```

### API Endpoints

**Authentication:**
- `POST /api/v1/auth/register` - Create account
- `POST /api/v1/auth/login` - Get JWT token
- `GET /api/v1/auth/me` - Get current user

**Stations:**
- `GET /api/v1/stations` - List all stations
- `POST /api/v1/stations` - Create station (admin)
- `GET /api/v1/stations/:id` - Get station details
- `PATCH /api/v1/stations/:id` - Update station (admin)
- `DELETE /api/v1/stations/:id` - Delete station (admin)
- `POST /api/v1/stations/:id/start` - Start broadcasting (admin)
- `POST /api/v1/stations/:id/stop` - Stop broadcasting (admin)
- `POST /api/v1/stations/:id/skip` - Skip current track (admin)
- `GET /api/v1/stations/:id/nowplaying` - Get now playing info

**Streaming:**
- `GET /api/v1/navidrome/stream/:track_id` - Stream audio (proxied to Navidrome)
- `GET /api/v1/navidrome/cover/:track_id` - Get album art (proxied to Navidrome)

### Security Considerations

**Authentication:**
- Passwords hashed with Argon2 (memory-hard, resistant to GPU attacks)
- JWTs signed with HS256 and configurable secret
- Tokens expire after 7 days
- No refresh token mechanism (re-login required)

**Authorization:**
- Role-based access control (user vs admin)
- Admin-only endpoints protected by middleware
- Station control limited to admin users

**Navidrome Integration:**
- Credentials stored in environment variables
- Authentication with Navidrome on backend only
- Client never sees Navidrome credentials
- All music streaming proxied through backend

**CORS:**
- Currently allows all origins in development
- Should be restricted in production

### Performance Optimizations

**Caching:**
- Track selection results cached in Redis
- Album art URLs cached
- Now playing info cached with TTL

**Database:**
- Indexed on frequently queried fields (station.path, user.username)
- Connection pooling via SQLx
- Prepared statements for all queries

**Frontend:**
- Static files pre-compressed (gzip/brotli)
- Embedded in binary (no separate file server needed)
- Media Session API reduces polling need

### Deployment

**Single Binary:**
- Frontend built to static files
- Embedded in Rust binary using `include_dir!` macro
- No separate web server needed
- Simplified deployment (just copy binary)

**Docker:**
- Multi-stage build (Node for frontend, Rust for backend)
- Multi-architecture support (amd64, arm64)
- Minimal runtime image based on Debian slim

**Dependencies:**
- PostgreSQL 16+ (persistent data)
- Redis 7+ (caching)
- Navidrome (music library)

## 💬 Support

Need help?
- Open an issue on GitHub
- Check the troubleshooting section above

---

**Built with ❤️ using Rust and Svelte**

