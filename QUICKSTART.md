# Quick Start Guide

## What Was Built

A complete, production-ready Navidrome Radio Station Platform with:

### ✅ Backend (Rust)
- **Authentication System**: JWT-based auth with Argon2 password hashing
- **RESTful API**: Complete CRUD operations for stations and users
- **Station Manager**: Manages multiple concurrent radio stations
- **AI Curation Engine**: Intelligent track selection (with random fallback)
- **Navidrome Integration**: Full Subsonic API client
- **Database**: PostgreSQL with migrations
- **Caching**: Redis for session and state management
- **Streaming**: Direct proxy to Navidrome audio streams

### ✅ Frontend (SvelteKit)
- **Responsive Design**: Mobile-first UI with Tailwind CSS
- **Authentication Pages**: Login and registration
- **Station List**: Browse all available stations
- **Audio Player**: Listen to stations with play/pause controls
- **Admin Dashboard**: Create, start, stop, and manage stations
- **Real-time Updates**: Polling for now-playing information

### ✅ Infrastructure
- **Docker Compose**: Full stack deployment
- **Migrations**: Automated database schema management
- **Configuration**: Environment-based configuration
- **Documentation**: Complete README and setup guides

## Running the Application

### Prerequisites

1. **Docker and Docker Compose** (easiest)
   - Install from: https://www.docker.com/get-started

2. **OR Manual Setup**:
   - Rust 1.75+ (https://rustup.rs/)
   - Node.js 20+ (https://nodejs.org/)
   - PostgreSQL 16+
   - Redis 7+
   - Navidrome server

### Option 1: Docker (Recommended)

1. **Start all services**:
   ```bash
   docker-compose up -d
   ```

2. **Check logs**:
   ```bash
   docker-compose logs -f
   ```

3. **Access services**:
   - Navidrome: http://localhost:4533
   - Backend API: http://localhost:8000

4. **Set up Navidrome**:
   - First time? Create an admin account in Navidrome
   - Add your music library
   - Update `.env` with your Navidrome credentials

### Option 2: Manual Setup

1. **Start infrastructure**:
   ```bash
   docker-compose up -d postgres redis navidrome
   ```

2. **Run backend**:
   ```bash
   cd backend
   cargo run
   # Server starts on http://localhost:8000
   ```

3. **Run frontend** (in new terminal):
   ```bash
   cd frontend
   npm install
   npm run dev
   # Frontend starts on http://localhost:5173
   ```

## First Steps

### 1. Create Admin Account

**Using the Frontend**:
- Navigate to http://localhost:5173/register
- Create your first user (automatically gets admin role)
- Login with your credentials

**Using API**:
```bash
curl -X POST http://localhost:8000/api/v1/auth/register \
  -H "Content-Type: application/json" \
  -d '{
    "username": "admin",
    "email": "admin@example.com",
    "password": "securepassword123"
  }'
```

### 2. Create Your First Station

1. **Login** as admin
2. **Navigate to Admin Dashboard** (/admin)
3. **Click "Create New Station"**
4. **Fill in details**:
   - **Name**: "Rock Classics"
   - **Path**: "rock-classics"
   - **Description**: "Classic rock from the 70s, 80s, and 90s. Guitar-driven anthems and timeless hits."
   - **Genres**: "Rock, Classic Rock, Hard Rock"
5. **Click "Create Station"**
6. **Click "Start"** to begin broadcasting

### 3. Listen to Your Station

1. **Navigate to**: http://localhost:5173/station/rock-classics
2. **Click the play button** to start listening
3. **Share the URL** with others to listen together!

## Testing the Application

### Test User Registration
```bash
curl -X POST http://localhost:8000/api/v1/auth/register \
  -H "Content-Type: application/json" \
  -d '{
    "username": "testuser",
    "email": "test@example.com",
    "password": "testpass123"
  }'
```

### Test Login
```bash
curl -X POST http://localhost:8000/api/v1/auth/login \
  -H "Content-Type: application/json" \
  -d '{
    "username": "testuser",
    "password": "testpass123"
  }'
```

### Test Creating a Station
```bash
# Get token from login response, then:
curl -X POST http://localhost:8000/api/v1/stations \
  -H "Authorization: Bearer YOUR_TOKEN_HERE" \
  -H "Content-Type: application/json" \
  -d '{
    "path": "indie-vibes",
    "name": "Indie Vibes",
    "description": "Chill indie music with dreamy vocals and mellow instrumentals",
    "genres": ["Indie Rock", "Indie Pop", "Alternative"]
  }'
```

### Test Listing Stations
```bash
curl http://localhost:8000/api/v1/stations
```

## Architecture Overview

```
Frontend (Svelte) ─────► Backend (Rust/Axum) ─────► Navidrome
    :5173                    :8000                    :4533
                              │
                   ┌──────────┼──────────┐
                   │                     │
              PostgreSQL              Redis
                :5432                 :6379
```

## Project Structure

```
navidrome-radio/
├── backend/
│   ├── src/
│   │   ├── main.rs              # Application entry point
│   │   ├── config.rs            # Configuration
│   │   ├── error.rs             # Error handling
│   │   ├── models/              # Data models
│   │   ├── services/            # Business logic
│   │   └── api/                 # API routes
│   ├── migrations/              # Database migrations
│   └── Cargo.toml               # Rust dependencies
├── frontend/
│   ├── src/
│   │   ├── routes/              # Pages
│   │   ├── lib/                 # Components & utilities
│   │   └── app.html             # HTML shell
│   └── package.json             # Node dependencies
├── docker-compose.yml           # Docker services
├── .env                         # Configuration
└── README.md                    # Documentation
```

## API Endpoints

### Authentication
- `POST /api/v1/auth/register` - Register new user
- `POST /api/v1/auth/login` - Login user
- `GET /api/v1/auth/me` - Get current user

### Stations
- `GET /api/v1/stations` - List all stations
- `POST /api/v1/stations` - Create station (admin)
- `GET /api/v1/stations/:id` - Get station
- `PATCH /api/v1/stations/:id` - Update station (admin)
- `DELETE /api/v1/stations/:id` - Delete station (admin)
- `POST /api/v1/stations/:id/start` - Start station (admin)
- `POST /api/v1/stations/:id/stop` - Stop station (admin)
- `POST /api/v1/stations/:id/skip` - Skip track (admin)
- `GET /api/v1/stations/:id/nowplaying` - Get now playing

### Streaming
- `GET /api/v1/api/stream/:track_id` - Stream audio
- `GET /api/v1/api/cover/:track_id` - Get album art

## Features Implemented

### Core Features
- ✅ User authentication (JWT + Argon2)
- ✅ Admin vs Listener roles
- ✅ Create/edit/delete stations
- ✅ Start/stop stations
- ✅ Skip tracks (admin only)
- ✅ Multi-genre support
- ✅ Track curation (random with AI fallback)
- ✅ Direct Navidrome streaming
- ✅ Now playing information
- ✅ Listener count tracking

### UI Features
- ✅ Responsive mobile design
- ✅ Dark theme
- ✅ Station cards with live indicators
- ✅ Audio player with play/pause
- ✅ Admin dashboard
- ✅ Station creation form
- ✅ Real-time polling for updates

## Next Steps

1. **Configure Navidrome**:
   - Add your music library
   - Update `.env` with real credentials
   - Ensure Navidrome has scanned your library

2. **Customize Stations**:
   - Create stations for different genres
   - Write detailed descriptions for better AI curation
   - Test different track selection modes

3. **Add AI Features** (Optional):
   - Get Anthropic API key for Claude
   - Get OpenAI API key for embeddings
   - Update `.env` with API keys
   - Switch stations to `ai_contextual` mode

4. **Deploy to Production**:
   - Change `JWT_SECRET` to secure random value
   - Use strong database passwords
   - Set up HTTPS with reverse proxy
   - Configure firewall rules

## Troubleshooting

### Backend won't start
```bash
# Check if ports are available
lsof -i :8000
lsof -i :5432
lsof -i :6379

# Check Docker services
docker-compose ps

# View logs
docker-compose logs backend
```

### Database connection failed
```bash
# Restart PostgreSQL
docker-compose restart postgres

# Check if database exists
docker-compose exec postgres psql -U postgres -l
```

### No tracks playing
```bash
# Check Navidrome is running
curl http://localhost:4533/ping

# Verify credentials in .env
# Ensure Navidrome has scanned music library
```

### Frontend can't connect
```bash
# Check backend is running
curl http://localhost:8000/api/v1/stations

# Check CORS settings
# Verify proxy in vite.config.ts
```

## Development Commands

### Backend
```bash
cd backend
cargo check        # Check for errors
cargo build        # Build
cargo run          # Run
cargo test         # Run tests
```

### Frontend
```bash
cd frontend
npm install        # Install dependencies
npm run dev        # Development server
npm run build      # Production build
npm run preview    # Preview production build
```

### Database
```bash
# Create migration
cd backend
sqlx migrate add migration_name

# Run migrations
sqlx migrate run

# Revert migration
sqlx migrate revert
```

## Success Criteria

Your application is working correctly when:

1. ✅ You can register and login
2. ✅ You can create a station in the admin dashboard
3. ✅ You can start a station
4. ✅ You can navigate to `/station/your-path` and see the player
5. ✅ You can click play and hear audio
6. ✅ The "Now Playing" information updates
7. ✅ Admin users can skip tracks
8. ✅ Multiple users can listen simultaneously

## Support

For issues:
- Check the README.md for detailed documentation
- Review the SPECIFICATION.md for technical details
- Check Docker logs: `docker-compose logs -f`
- Verify `.env` configuration
- Ensure Navidrome is properly configured

---

**Built with Rust, Svelte, and ❤️**
