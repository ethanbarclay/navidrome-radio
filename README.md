# Navidrome Radio - AI-Powered Radio Station Platform

A high-performance radio station platform that transforms your Navidrome music library into multiple synchronized streaming radio stations. Built with Rust for maximum performance and Svelte for a modern, responsive mobile-optimized interface.

## Features

- **Multiple Radio Stations**: Create and manage multiple virtual radio stations
- **AI-Powered Curation**: Intelligent track selection based on station descriptions (optional)
- **Synchronized Playback**: All listeners hear the same track at the same time
- **Admin Controls**: Start/stop stations and skip tracks (admin only)
- **Responsive Design**: Works seamlessly on desktop, tablet, and mobile
- **Real-time Updates**: Live listener counts and now playing information
- **Navidrome Integration**: Seamless integration with your existing Navidrome server

## Tech Stack

### Backend
- **Rust** with Axum web framework
- **PostgreSQL** for data storage
- **Redis** for caching and state management
- **JWT** authentication with Argon2 password hashing
- **Navidrome API** integration

### Frontend
- **SvelteKit 2** with Svelte 5 (runes)
- **Tailwind CSS 4** for styling
- **TypeScript** for type safety
- **Mobile-first responsive design**

## Prerequisites

- **Docker and Docker Compose** (recommended for quick start)
- OR manually install:
  - Rust 1.75+ (https://rustup.rs/)
  - Node.js 20+ (https://nodejs.org/)
  - PostgreSQL 16+
  - Redis 7+
  - Navidrome server (https://www.navidrome.org/)

## Quick Start with Docker

1. **Clone the repository**
   ```bash
   git clone <your-repo-url>
   cd navidrome-radio
   ```

2. **Set up environment variables**
   ```bash
   cp .env.example .env
   ```

3. **Edit `.env` with your settings**
   ```bash
   # Required: Set your Navidrome credentials
   NAVIDROME_URL=http://navidrome:4533
   NAVIDROME_USER=admin
   NAVIDROME_PASSWORD=your-password

   # Optional: Set AI API keys for intelligent curation
   ANTHROPIC_API_KEY=your-key-here
   OPENAI_API_KEY=your-key-here

   # Change this in production!
   JWT_SECRET=your-super-secret-jwt-key-change-in-production
   ```

4. **Start the services**
   ```bash
   docker-compose up -d
   ```

5. **Wait for services to be ready** (check with `docker-compose logs -f`)

6. **Access the application**
   - Backend API: http://localhost:8000
   - Frontend will be available after building (see below)

## Manual Setup

### Backend Setup

1. **Install Rust**
   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   ```

2. **Start PostgreSQL and Redis**
   ```bash
   docker-compose up -d postgres redis
   ```

3. **Set up environment variables**
   ```bash
   cp .env.example .env
   # Edit .env with your configuration
   ```

4. **Run database migrations**
   ```bash
   cd backend
   cargo install sqlx-cli
   sqlx migrate run
   ```

5. **Start the backend server**
   ```bash
   cargo run
   # Or for production build:
   cargo build --release
   ./target/release/navidrome-radio
   ```

   The backend will be available at `http://localhost:8000`

### Frontend Setup

1. **Install dependencies**
   ```bash
   cd frontend
   npm install
   ```

2. **Start development server**
   ```bash
   npm run dev
   ```

   The frontend will be available at `http://localhost:5173`

3. **Build for production**
   ```bash
   npm run build
   npm run preview
   ```

## Usage

### Creating Your First Station

1. **Register an account**
   - Navigate to `/register`
   - Create your account (first user is automatically admin)

2. **Create a station**
   - Go to `/admin` (admin only)
   - Click "Create New Station"
   - Fill in:
     - **Station Name**: Display name (e.g., "Chill Indie Vibes")
     - **URL Path**: URL-friendly path (e.g., "chill-indie")
     - **Description**: Rich description for AI curation
     - **Genres**: Comma-separated genres (e.g., "Indie Rock, Alternative")

3. **Start the station**
   - Click "Start" button in the admin dashboard
   - The station will begin playing tracks automatically

4. **Listen**
   - Navigate to `/station/your-path` (e.g., `/station/chill-indie`)
   - Click play to start listening
   - All listeners hear the same track simultaneously

### Admin Features

As an admin, you can:
- Create, edit, and delete stations
- Start and stop stations
- Skip tracks (all listeners will hear the new track)
- View all active stations

### Regular User Features

Regular listeners can:
- Browse all available stations
- Listen to active stations
- See now playing information
- View listener counts

## Architecture Overview

```
┌─────────────────────────────────────────┐
│           Frontend (SvelteKit)          │
│  - Station List                         │
│  - Player UI                            │
│  - Admin Dashboard                      │
└─────────────────┬───────────────────────┘
                  │ HTTP/REST API
┌─────────────────▼───────────────────────┐
│           Backend (Rust/Axum)           │
│  - Authentication (JWT)                 │
│  - Station Manager                      │
│  - Track Curation                       │
│  - Navidrome Client                     │
└─────────┬───────────────┬───────────────┘
          │               │
    ┌─────▼─────┐   ┌────▼─────┐
    │PostgreSQL │   │  Redis   │
    │(Stations) │   │ (State)  │
    └───────────┘   └──────────┘
          │
    ┌─────▼──────┐
    │ Navidrome  │
    │  (Music)   │
    └────────────┘
```

## Configuration

### Environment Variables

| Variable | Description | Default | Required |
|----------|-------------|---------|----------|
| `DATABASE_URL` | PostgreSQL connection string | `postgres://postgres:postgres@localhost:5432/navidrome_radio` | Yes |
| `REDIS_URL` | Redis connection string | `redis://localhost:6379` | Yes |
| `NAVIDROME_URL` | Navidrome server URL | - | Yes |
| `NAVIDROME_USER` | Navidrome username | - | Yes |
| `NAVIDROME_PASSWORD` | Navidrome password | - | Yes |
| `ANTHROPIC_API_KEY` | Claude API key (optional) | - | No |
| `OPENAI_API_KEY` | OpenAI API key (optional) | - | No |
| `JWT_SECRET` | Secret for JWT tokens | - | Yes |
| `SERVER_HOST` | Server bind address | `0.0.0.0` | No |
| `SERVER_PORT` | Server port | `8000` | No |
| `RUST_LOG` | Logging level | `info` | No |

### Station Configuration

Each station supports the following configuration options:

- **Bitrate**: Audio quality (128, 192, or 256 kbps)
- **Track Selection Mode**:
  - `random`: Random selection from genres
  - `ai_contextual`: AI-powered based on description (requires API key)
  - `ai_embeddings`: Embedding-based similarity (requires API key)
  - `hybrid`: Mix of AI and random
- **Duration Filters**: Min/max track duration
- **Explicit Content**: Allow/disallow explicit tracks

## Troubleshooting

### Backend won't start
- Check that PostgreSQL and Redis are running
- Verify your `.env` file has correct credentials
- Check logs: `RUST_LOG=debug cargo run`

### Can't connect to Navidrome
- Verify `NAVIDROME_URL` is accessible from the backend
- Check Navidrome username and password
- Ensure Navidrome is running and healthy

### No tracks found
- Verify your Navidrome library has tracks in the specified genres
- Check Navidrome has completed scanning your music library
- Try broader genre search terms

### Frontend can't connect to backend
- Ensure backend is running on port 8000
- Check CORS settings if accessing from different domain
- Verify proxy configuration in `vite.config.ts`

## Development

### Running Tests (Backend)
```bash
cd backend
cargo test
```

### Database Migrations
```bash
cd backend
# Create new migration
sqlx migrate add migration_name

# Run migrations
sqlx migrate run

# Revert last migration
sqlx migrate revert
```

### Code Formatting
```bash
# Backend
cd backend
cargo fmt
cargo clippy

# Frontend
cd frontend
npm run check
```

## Deployment

### Production Checklist

1. **Security**
   - [ ] Change `JWT_SECRET` to a strong random value
   - [ ] Use strong database passwords
   - [ ] Enable HTTPS (use reverse proxy like Caddy)
   - [ ] Set up firewall rules

2. **Performance**
   - [ ] Build backend in release mode: `cargo build --release`
   - [ ] Build frontend: `npm run build`
   - [ ] Configure Redis max memory
   - [ ] Set up database connection pooling

3. **Monitoring**
   - [ ] Set up logging aggregation
   - [ ] Configure health check endpoints
   - [ ] Monitor database and Redis performance

### Docker Deployment

```bash
# Build and run in production mode
docker-compose -f docker-compose.yml up -d

# View logs
docker-compose logs -f

# Stop services
docker-compose down
```

## Roadmap

- [ ] WebSocket support for real-time updates
- [ ] Playlist history and analytics
- [ ] User favorites and recommendations
- [ ] Scheduled programming
- [ ] Mobile apps (React Native / Flutter)
- [ ] Social features (chat, reactions)
- [ ] Advanced AI curation with embeddings
- [ ] HLS streaming with live transcoding
- [ ] Multi-server federation

## Contributing

Contributions are welcome! Please:

1. Fork the repository
2. Create a feature branch
3. Make your changes
4. Add tests if applicable
5. Submit a pull request

## License

This project is licensed under the MIT License - see LICENSE file for details.

## Acknowledgments

- [Navidrome](https://www.navidrome.org/) - Open-source music server
- [Axum](https://github.com/tokio-rs/axum) - Rust web framework
- [SvelteKit](https://kit.svelte.dev/) - Frontend framework
- [Tailwind CSS](https://tailwindcss.com/) - CSS framework

## Support

For issues and questions:
- Open an issue on GitHub
- Check existing documentation
- Review troubleshooting section

---

Built with ❤️ using Rust and Svelte
