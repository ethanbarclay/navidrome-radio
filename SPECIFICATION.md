# Navidrome Radio Station Builder - Technical Specification

**Version:** 1.0.0
**Date:** 2025-11-23
**Status:** Draft

## 1. Executive Summary

A high-performance, AI-powered radio station platform that transforms a Navidrome music library into multiple synchronized streaming radio stations. Built with Rust for maximum performance and memory safety, featuring real-time synchronized playback across all connected clients, LLM-powered track selection, and a responsive mobile-optimized web interface.

## 2. System Overview

### 2.1 Core Concept

The system enables administrators to create and manage multiple virtual radio stations, each with unique descriptions and characteristics. An LLM analyzes station descriptions and selects appropriate tracks from a Navidrome library. All listeners connecting to a station hear the same audio at the same timestamp, creating a shared listening experience.

### 2.2 Key Differentiators

- **Synchronized Streaming**: All listeners hear identical audio at identical timestamps
- **AI-Powered Curation**: LLM selects tracks based on natural language station descriptions
- **Zero Client Controls**: Immersive radio experience with no playback controls for listeners
- **High Performance**: Rust-based backend ensures minimal latency and maximum throughput
- **Responsive Mobile Design**: Optimized UI that works seamlessly on desktop, tablet, and mobile devices

## 3. System Architecture

### 3.1 Architecture Pattern

**Event-Driven Microservices Architecture** with the following components:

```
┌─────────────────────────────────────────────────────────────┐
│                         Client Layer                         │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐       │
│  │   Web PWA    │  │ iOS Safari   │  │Android Chrome│       │
│  └──────────────┘  └──────────────┘  └──────────────┘       │
└─────────────────────────────────────────────────────────────┘
                            │ HTTPS/WebSocket/HLS
┌─────────────────────────────────────────────────────────────┐
│                      API Gateway Layer                       │
│                    (axum HTTP server)                        │
└─────────────────────────────────────────────────────────────┘
                            │
        ┌───────────────────┼───────────────────┐
        │                   │                   │
┌───────▼──────┐   ┌────────▼────────┐   ┌─────▼──────┐
│   Station    │   │    Streaming    │   │    Auth    │
│  Management  │   │     Engine      │   │  Service   │
│   Service    │   │                 │   │            │
└───────┬──────┘   └────────┬────────┘   └────────────┘
        │                   │
        │          ┌────────▼────────┐
        │          │   AI Curation   │
        └──────────►     Engine      │
                   │                 │
                   └────────┬────────┘
                            │
                   ┌────────▼────────┐
                   │    Navidrome    │
                   │   API Client    │
                   └────────┬────────┘
                            │
┌─────────────────────────────────────────────────────────────┐
│                      Data Layer                              │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐    │
│  │PostgreSQL│  │  Redis   │  │ DiskCache│  │Navidrome │    │
│  │(Metadata)│  │ (State)  │  │ (Audio)  │  │  Server  │    │
│  └──────────┘  └──────────┘  └──────────┘  └──────────┘    │
└─────────────────────────────────────────────────────────────┘
```

### 3.2 Technology Stack

#### Backend Core
- **Language**: Rust (stable channel, 2021 edition)
- **Web Framework**: `axum` 0.7+ (high-performance async web framework)
- **Async Runtime**: `tokio` 1.35+ (multi-threaded async runtime)
- **Database**: PostgreSQL 16+ with `sqlx` (async SQL toolkit)
- **Caching**: Redis 7+ with `redis-rs` (state management, session storage)
- **Audio Processing**: `symphonia` (pure Rust audio decoding)

#### Streaming Infrastructure
- **Protocol**: HLS (HTTP Live Streaming) for broad compatibility
- **Transcoding**: `ffmpeg-next` (Rust bindings to FFmpeg)
- **Format**: AAC-LC 128-256kbps in MPEG-TS containers
- **Segmentation**: 2-second segments for low latency

#### AI/LLM Integration
- **Primary**: Anthropic Claude API (Claude 3.5 Sonnet)
- **Fallback**: OpenAI GPT-4 Turbo
- **Embeddings**: `text-embedding-3-large` for semantic search
- **Vector Store**: `pgvector` extension for PostgreSQL

#### Frontend
- **Framework**: Svelte 5 (runes mode) with SvelteKit 2
- **Build Tool**: Vite 5+ with SWC
- **Styling**: Tailwind CSS 4 (oxide engine) with responsive utilities
- **State Management**: Svelte stores + TanStack Query
- **Audio Player**: `hls.js` for HLS playback
- **Mobile Optimization**: CSS media queries, touch-friendly controls, viewport optimization

#### DevOps & Infrastructure
- **Containerization**: Docker with multi-stage builds
- **Orchestration**: Docker Compose (dev), Kubernetes (production)
- **Reverse Proxy**: Caddy 2 (automatic HTTPS)
- **Monitoring**: Prometheus + Grafana
- **Logging**: `tracing` + `tracing-subscriber` (structured logging)
- **CI/CD**: GitHub Actions

## 4. Detailed Component Specifications

### 4.1 Station Management Service

**Responsibility**: CRUD operations for radio stations

#### Data Model

```rust
struct Station {
    id: Uuid,
    path: String,              // URL path (e.g., "rock", "jazz-evening")
    name: String,              // Display name
    description: String,       // Rich description for LLM
    genres: Vec<String>,       // Navidrome genre filters
    mood_tags: Vec<String>,    // Additional curation hints
    created_by: Uuid,          // User ID
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    active: bool,              // Broadcasting status
    config: StationConfig,
}

struct StationConfig {
    bitrate: u32,              // 128, 192, 256 kbps
    sample_rate: u32,          // 44100 or 48000 Hz
    crossfade_ms: u32,         // Crossfade duration (0-10000ms)
    track_selection_mode: SelectionMode, // AI, Random, Queue
    min_track_duration: u32,   // Minimum seconds
    max_track_duration: u32,   // Maximum seconds
    explicit_content: bool,    // Allow explicit tracks
}

enum SelectionMode {
    AIContextual,              // LLM-powered selection
    AIEmbeddings,              // Embedding-based similarity
    Random,                    // Pure random from genres
    Hybrid,                    // Mix of AI and random
}
```

#### API Endpoints

```
POST   /api/v1/stations          - Create station (admin)
GET    /api/v1/stations          - List all stations
GET    /api/v1/stations/:id      - Get station details
PATCH  /api/v1/stations/:id      - Update station (admin)
DELETE /api/v1/stations/:id      - Delete station (admin)
POST   /api/v1/stations/:id/start   - Start broadcasting (admin)
POST   /api/v1/stations/:id/stop    - Stop broadcasting (admin)
POST   /api/v1/stations/:id/skip    - Skip current track (admin)
GET    /api/v1/stations/:id/nowplaying - Current track info
```

### 4.2 Streaming Engine

**Responsibility**: Synchronized audio delivery to all clients

#### Architecture

```rust
struct StreamingEngine {
    stations: Arc<RwLock<HashMap<Uuid, ActiveStation>>>,
    encoder_pool: Arc<EncoderPool>,
    segment_cache: Arc<SegmentCache>,
}

struct ActiveStation {
    id: Uuid,
    current_track: Arc<RwLock<Option<Track>>>,
    start_timestamp: DateTime<Utc>,  // When current track started
    playlist_queue: Arc<RwLock<VecDeque<Track>>>,
    listeners: Arc<AtomicUsize>,
    encoder_handle: JoinHandle<()>,
}

struct EncoderPool {
    max_concurrent: usize,
    available: Semaphore,
}

struct SegmentCache {
    storage: Arc<DiskCache>,
    memory_cache: Arc<Mutex<LruCache<SegmentKey, Bytes>>>,
    max_memory_mb: usize,
}
```

#### Synchronization Strategy

1. **Master Clock**: Each station maintains a monotonic start time
2. **Segment Numbering**: Sequential segment IDs synced to wall clock
3. **Client Sync**: Clients calculate which segment to request based on current time
4. **Catchup Logic**: New clients join at current live position (no rewind)
5. **Drift Correction**: Clients measure latency and adjust playback speed (0.95x-1.05x)

#### HLS Manifest Generation

```rust
async fn generate_master_playlist(station_id: Uuid) -> Result<String, Error> {
    let station = get_station(station_id).await?;

    Ok(format!(
        "#EXTM3U\n\
         #EXT-X-VERSION:7\n\
         #EXT-X-INDEPENDENT-SEGMENTS\n\
         #EXT-X-STREAM-INF:BANDWIDTH={},CODECS=\"mp4a.40.2\"\n\
         /stream/{}/playlist.m3u8",
        station.config.bitrate * 1000,
        station.path
    ))
}

async fn generate_media_playlist(
    station_id: Uuid,
    now: DateTime<Utc>
) -> Result<String, Error> {
    let station = get_active_station(station_id).await?;
    let current_segment = calculate_segment_number(&station, now);

    // Generate sliding window of 6 segments (12 seconds @ 2s/segment)
    let segments = (current_segment - 3)..=(current_segment + 2);

    let mut playlist = String::from(
        "#EXTM3U\n\
         #EXT-X-VERSION:7\n\
         #EXT-X-TARGETDURATION:2\n\
         #EXT-X-MEDIA-SEQUENCE:{}\n"
    );

    for seg in segments {
        playlist.push_str(&format!(
            "#EXTINF:2.0,\n\
             /stream/{}/segment/{}.ts\n",
            station.path, seg
        ));
    }

    Ok(playlist)
}
```

#### Transcoding Pipeline

```rust
async fn transcode_and_segment(
    track: Track,
    station_config: StationConfig,
    output_tx: mpsc::Sender<Segment>
) -> Result<(), Error> {
    use symphonia::core::*;

    // 1. Fetch audio from Navidrome
    let audio_stream = navidrome_client.stream_track(track.id).await?;

    // 2. Decode using Symphonia
    let decoder = codecs::CODEC_REGISTRY
        .make(&track.codec_params, &DecoderOptions::default())?;

    // 3. Resample to target rate if needed
    let resampler = if decoder.sample_rate() != station_config.sample_rate {
        Some(create_resampler(decoder.sample_rate(), station_config.sample_rate))
    } else {
        None
    };

    // 4. Encode to AAC via FFmpeg
    let encoder = ffmpeg::encoder::find(ffmpeg::codec::Id::AAC)
        .expect("AAC encoder")
        .audio()?
        .bit_rate(station_config.bitrate * 1000)
        .rate(station_config.sample_rate as i32)
        .build()?;

    // 5. Segment into 2-second MPEG-TS chunks
    let mut segmenter = MpegTsSegmenter::new(2.0);

    // 6. Process audio frames
    loop {
        let packet = match audio_stream.next_packet()? {
            Some(p) => p,
            None => break,
        };

        let decoded = decoder.decode(&packet)?;
        let resampled = resampler.map(|r| r.process(decoded)).unwrap_or(decoded);
        let encoded = encoder.encode(&resampled)?;

        if let Some(segment) = segmenter.add_data(encoded)? {
            output_tx.send(segment).await?;
        }
    }

    Ok(())
}
```

### 4.3 AI Curation Engine

**Responsibility**: Intelligent track selection using LLM and embeddings

#### Component Architecture

```rust
struct AICurationEngine {
    llm_client: Arc<AnthropicClient>,
    embedding_client: Arc<OpenAIClient>,
    vector_store: Arc<PgVectorStore>,
    navidrome_client: Arc<NavidromeClient>,
    cache: Arc<RedisClient>,
}

struct Track {
    id: String,
    title: String,
    artist: String,
    album: String,
    genre: Vec<String>,
    year: Option<u32>,
    duration: u32,
    path: String,
    metadata: HashMap<String, String>,
    embedding: Option<Vec<f32>>,  // 3072-dim vector
}
```

#### Track Selection Flow

```rust
async fn select_next_track(
    &self,
    station: &Station,
    recent_tracks: Vec<Track>,  // Last 20 tracks to avoid repetition
) -> Result<Track, Error> {
    match station.config.track_selection_mode {
        SelectionMode::AIContextual => {
            self.select_with_llm(station, recent_tracks).await
        }
        SelectionMode::AIEmbeddings => {
            self.select_with_embeddings(station, recent_tracks).await
        }
        SelectionMode::Random => {
            self.select_random(station, recent_tracks).await
        }
        SelectionMode::Hybrid => {
            // 70% AI, 30% random for variety
            if rand::random::<f32>() < 0.7 {
                self.select_with_llm(station, recent_tracks).await
            } else {
                self.select_random(station, recent_tracks).await
            }
        }
    }
}
```

#### LLM-Powered Selection

```rust
async fn select_with_llm(
    &self,
    station: &Station,
    recent_tracks: Vec<Track>,
) -> Result<Track, Error> {
    // 1. Get candidate tracks from Navidrome (filtered by genres)
    let candidates = self.navidrome_client
        .search_tracks(&station.genres, 100)
        .await?;

    // 2. Filter out recently played
    let recent_ids: HashSet<_> = recent_tracks.iter().map(|t| &t.id).collect();
    let candidates: Vec<_> = candidates
        .into_iter()
        .filter(|t| !recent_ids.contains(&t.id))
        .collect();

    // 3. Build context for LLM
    let context = format!(
        "Station: {}\nDescription: {}\nMood: {}\n\nRecent tracks:\n{}",
        station.name,
        station.description,
        station.mood_tags.join(", "),
        recent_tracks.iter()
            .take(5)
            .map(|t| format!("- {} by {}", t.title, t.artist))
            .collect::<Vec<_>>()
            .join("\n")
    );

    // 4. Sample 20 candidates and ask LLM to rank
    let sample = candidates
        .choose_multiple(&mut rand::thread_rng(), 20)
        .cloned()
        .collect::<Vec<_>>();

    let prompt = format!(
        "{}\n\nHere are {} candidate tracks. Respond with ONLY the number \
         (0-{}) of the BEST track that fits this radio station's vibe:\n\n{}",
        context,
        sample.len(),
        sample.len() - 1,
        sample.iter()
            .enumerate()
            .map(|(i, t)| format!(
                "{}: \"{}\" by {} [{}] ({})",
                i,
                t.title,
                t.artist,
                t.genre.join(", "),
                t.year.map(|y| y.to_string()).unwrap_or_default()
            ))
            .collect::<Vec<_>>()
            .join("\n")
    );

    let response = self.llm_client
        .complete(&prompt, 10, 0.8)  // max_tokens, temperature
        .await?;

    // 5. Parse response and return selected track
    let index: usize = response.trim().parse()
        .map_err(|_| Error::InvalidLLMResponse)?;

    sample.get(index)
        .cloned()
        .ok_or(Error::TrackNotFound)
}
```

#### Embedding-Based Selection

```rust
async fn select_with_embeddings(
    &self,
    station: &Station,
    recent_tracks: Vec<Track>,
) -> Result<Track, Error> {
    // 1. Generate embedding for station description
    let station_embedding = self.get_or_create_station_embedding(station).await?;

    // 2. Query vector store for similar tracks
    let similar_tracks = self.vector_store
        .query_similar(
            &station_embedding,
            &station.genres,
            100,  // limit
            0.7   // minimum similarity
        )
        .await?;

    // 3. Filter out recent tracks
    let recent_ids: HashSet<_> = recent_tracks.iter().map(|t| &t.id).collect();
    let candidates: Vec<_> = similar_tracks
        .into_iter()
        .filter(|t| !recent_ids.contains(&t.id))
        .collect();

    // 4. Select weighted random (higher similarity = higher probability)
    select_weighted_random(candidates)
}

async fn get_or_create_station_embedding(
    &self,
    station: &Station
) -> Result<Vec<f32>, Error> {
    // Check cache first
    let cache_key = format!("station_embedding:{}", station.id);
    if let Some(cached) = self.cache.get(&cache_key).await? {
        return Ok(cached);
    }

    // Generate new embedding
    let text = format!(
        "{} {} {}",
        station.description,
        station.mood_tags.join(" "),
        station.genres.join(" ")
    );

    let embedding = self.embedding_client
        .create_embedding(&text)
        .await?;

    // Cache for 1 hour
    self.cache.set(&cache_key, &embedding, 3600).await?;

    Ok(embedding)
}
```

#### Track Embedding Generation (Background Job)

```rust
async fn generate_track_embeddings_job(&self) -> Result<(), Error> {
    loop {
        // Find tracks without embeddings
        let tracks = sqlx::query_as::<_, Track>(
            "SELECT * FROM tracks WHERE embedding IS NULL LIMIT 100"
        )
        .fetch_all(&self.db)
        .await?;

        if tracks.is_empty() {
            tokio::time::sleep(Duration::from_secs(300)).await;
            continue;
        }

        for track in tracks {
            // Create rich text representation
            let text = format!(
                "{} by {} from {} [{}] {}",
                track.title,
                track.artist,
                track.album,
                track.genre.join(", "),
                track.year.map(|y| y.to_string()).unwrap_or_default()
            );

            let embedding = self.embedding_client
                .create_embedding(&text)
                .await?;

            // Store in database
            sqlx::query(
                "UPDATE tracks SET embedding = $1 WHERE id = $2"
            )
            .bind(&embedding)
            .bind(&track.id)
            .execute(&self.db)
            .await?;

            // Rate limiting
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }
}
```

### 4.4 Authentication & Authorization

**Responsibility**: User management and access control

#### User Model

```rust
#[derive(Debug, Clone)]
struct User {
    id: Uuid,
    username: String,
    email: String,
    password_hash: String,  // Argon2id
    role: UserRole,
    created_at: DateTime<Utc>,
    last_login: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UserRole {
    Admin,      // Full access: create/edit stations, skip tracks
    Listener,   // Read-only: listen to stations
}
```

#### Authentication Flow

```rust
// JWT-based authentication
struct AuthService {
    secret_key: Vec<u8>,
    token_expiry: Duration,  // 7 days
}

impl AuthService {
    async fn login(
        &self,
        username: &str,
        password: &str
    ) -> Result<AuthResponse, Error> {
        // 1. Fetch user from database
        let user = User::find_by_username(username).await?
            .ok_or(Error::InvalidCredentials)?;

        // 2. Verify password
        argon2::verify_encoded(&user.password_hash, password.as_bytes())?;

        // 3. Generate JWT
        let claims = Claims {
            sub: user.id,
            role: user.role,
            exp: (Utc::now() + self.token_expiry).timestamp(),
        };

        let token = jsonwebtoken::encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(&self.secret_key)
        )?;

        Ok(AuthResponse {
            token,
            user: user.into(),
        })
    }
}
```

#### Authorization Middleware

```rust
async fn require_admin(
    State(auth): State<AuthService>,
    headers: HeaderMap,
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let token = headers
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or(StatusCode::UNAUTHORIZED)?;

    let claims = auth.verify_token(token)
        .map_err(|_| StatusCode::UNAUTHORIZED)?;

    if claims.role != UserRole::Admin {
        return Err(StatusCode::FORBIDDEN);
    }

    Ok(next.run(request).await)
}
```

### 4.5 Navidrome Integration

**Responsibility**: Interaction with Navidrome API

#### Client Implementation

```rust
struct NavidromeClient {
    base_url: String,
    username: String,
    token: String,
    salt: String,
    client: reqwest::Client,
}

impl NavidromeClient {
    fn new(base_url: String, username: String, password: String) -> Self {
        let salt = generate_random_salt();
        let token = md5::compute(format!("{}{}", password, salt)).to_hex();

        Self {
            base_url,
            username,
            token,
            salt,
            client: reqwest::Client::new(),
        }
    }

    async fn search_tracks(
        &self,
        genres: &[String],
        count: usize
    ) -> Result<Vec<Track>, Error> {
        let url = format!("{}/rest/search3", self.base_url);

        let response = self.client
            .get(&url)
            .query(&[
                ("u", self.username.as_str()),
                ("t", self.token.as_str()),
                ("s", self.salt.as_str()),
                ("v", "1.16.1"),
                ("c", "navidrome-radio"),
                ("f", "json"),
                ("query", &genres.join(" ")),
                ("songCount", &count.to_string()),
            ])
            .send()
            .await?
            .error_for_status()?
            .json::<SearchResponse>()
            .await?;

        Ok(response.search_result3.song)
    }

    async fn stream_track(&self, track_id: &str) -> Result<impl Stream<Item = Bytes>, Error> {
        let url = format!("{}/rest/stream", self.base_url);

        let response = self.client
            .get(&url)
            .query(&[
                ("u", self.username.as_str()),
                ("t", self.token.as_str()),
                ("s", self.salt.as_str()),
                ("v", "1.16.1"),
                ("c", "navidrome-radio"),
                ("id", track_id),
                ("format", "raw"),  // Original quality
            ])
            .send()
            .await?
            .error_for_status()?;

        Ok(response.bytes_stream())
    }

    async fn get_genres(&self) -> Result<Vec<Genre>, Error> {
        let url = format!("{}/rest/getGenres", self.base_url);

        let response = self.client
            .get(&url)
            .query(&[
                ("u", self.username.as_str()),
                ("t", self.token.as_str()),
                ("s", self.salt.as_str()),
                ("v", "1.16.1"),
                ("c", "navidrome-radio"),
                ("f", "json"),
            ])
            .send()
            .await?
            .error_for_status()?
            .json::<GenresResponse>()
            .await?;

        Ok(response.genres.genre)
    }
}
```

## 5. Frontend Architecture

### 5.1 Application Structure

```
src/
├── routes/
│   ├── +layout.svelte              # Root layout with auth
│   ├── +page.svelte                # Landing page / station list
│   ├── admin/
│   │   ├── +page.svelte            # Admin dashboard
│   │   ├── stations/
│   │   │   ├── +page.svelte        # Station management
│   │   │   ├── new/
│   │   │   │   └── +page.svelte    # Create station
│   │   │   └── [id]/
│   │   │       └── +page.svelte    # Edit station
│   ├── login/
│   │   └── +page.svelte            # Login page
│   └── [station]/
│       └── +page.svelte            # Station player page
├── lib/
│   ├── components/
│   │   ├── Player.svelte           # HLS player component
│   │   ├── StationCard.svelte      # Station preview card
│   │   ├── NowPlaying.svelte       # Current track display
│   │   └── AdminControls.svelte    # Skip/stop controls
│   ├── stores/
│   │   ├── auth.svelte.ts          # Auth state
│   │   ├── player.svelte.ts        # Player state
│   │   └── stations.svelte.ts      # Station data
│   ├── api/
│   │   └── client.ts               # API client
│   └── utils/
│       ├── hls.ts                  # HLS.js setup
│       └── sync.ts                 # Sync logic
└── app.html                        # HTML shell
```

### 5.2 Player Component (Mobile-Optimized)

```svelte
<script lang="ts">
  import Hls from 'hls.js';
  import { onMount, onDestroy } from 'svelte';

  let {
    stationPath,
    isAdmin = false,
    onSkip
  }: {
    stationPath: string;
    isAdmin?: boolean;
    onSkip?: () => void;
  } = $props();

  let audioElement: HTMLAudioElement | undefined = $state();
  let hls: Hls | undefined = $state();
  let nowPlaying = $state<Track | null>(null);
  let listeners = $state(0);
  let isPlaying = $state(false);

  onMount(async () => {
    if (!Hls.isSupported()) {
      throw new Error('HLS not supported');
    }

    hls = new Hls({
      enableWorker: true,
      lowLatencyMode: true,
      backBufferLength: 10,
      maxBufferLength: 12,
      maxMaxBufferLength: 15,
      liveSyncDurationCount: 3,
      liveMaxLatencyDurationCount: 5,
      liveDurationInfinity: true,
    });

    const manifestUrl = `/stream/${stationPath}/master.m3u8`;
    hls.loadSource(manifestUrl);

    if (audioElement) {
      hls.attachMedia(audioElement);
    }

    hls.on(Hls.Events.MANIFEST_PARSED, () => {
      audioElement?.play();
      isPlaying = true;
    });

    // Subscribe to now playing updates via WebSocket
    const ws = new WebSocket(`wss://${window.location.host}/ws/stations/${stationPath}`);

    ws.onmessage = (event) => {
      const data = JSON.parse(event.data);
      if (data.type === 'nowPlaying') {
        nowPlaying = data.track;
        listeners = data.listeners;
      }
    };

    return () => {
      hls?.destroy();
      ws.close();
    };
  });

  async function handleSkip() {
    if (!isAdmin || !onSkip) return;

    await fetch(`/api/v1/stations/${stationPath}/skip`, {
      method: 'POST',
      headers: {
        'Authorization': `Bearer ${getAuthToken()}`,
      },
    });

    onSkip?.();
  }
</script>

<!-- Responsive player with mobile-first design -->
<div class="player min-h-screen flex flex-col bg-gray-900 text-white p-4 md:p-8">
  <audio bind:this={audioElement} />

  {#if nowPlaying}
    <div class="now-playing flex-1 flex flex-col items-center justify-center space-y-4 md:space-y-6">
      <!-- Album art - responsive sizing -->
      <img
        src={nowPlaying.albumArt}
        alt={nowPlaying.album}
        class="w-64 h-64 md:w-80 md:h-80 lg:w-96 lg:h-96 rounded-lg shadow-2xl object-cover"
      />

      <!-- Track info - centered on mobile, larger text on desktop -->
      <div class="track-info text-center max-w-lg px-4">
        <h2 class="text-2xl md:text-3xl lg:text-4xl font-bold mb-2 truncate">
          {nowPlaying.title}
        </h2>
        <p class="text-lg md:text-xl text-gray-300 mb-1 truncate">
          {nowPlaying.artist}
        </p>
        <p class="text-sm md:text-base text-gray-400 truncate">
          {nowPlaying.album}
        </p>
      </div>
    </div>
  {/if}

  <!-- Stats and controls - fixed at bottom on mobile -->
  <div class="controls mt-8 space-y-4">
    <div class="stats flex items-center justify-center gap-2 text-sm md:text-base text-gray-400">
      <svg class="w-4 h-4 md:w-5 md:h-5" fill="currentColor" viewBox="0 0 20 20">
        <path d="M9 6a3 3 0 11-6 0 3 3 0 016 0zM17 6a3 3 0 11-6 0 3 3 0 016 0zM12.93 17c.046-.327.07-.66.07-1a6.97 6.97 0 00-1.5-4.33A5 5 0 0119 16v1h-6.07zM6 11a5 5 0 015 5v1H1v-1a5 5 0 015-5z"/>
      </svg>
      <span>{listeners} {listeners === 1 ? 'listener' : 'listeners'}</span>
    </div>

    {#if isAdmin}
      <!-- Touch-friendly skip button on mobile -->
      <button
        onclick={handleSkip}
        class="skip-btn w-full md:w-auto mx-auto block px-8 py-4 md:py-3 bg-blue-600 hover:bg-blue-700 rounded-lg font-semibold text-base md:text-lg transition-colors active:scale-95 touch-manipulation"
      >
        Skip Track
      </button>
    {/if}
  </div>
</div>

<style>
  /* Ensure text doesn't overflow on small screens */
  .truncate {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  /* Smooth transitions */
  .player {
    transition: all 0.3s ease;
  }

  /* Touch optimization */
  button {
    -webkit-tap-highlight-color: transparent;
    user-select: none;
  }
</style>
```

### 5.3 Mobile Optimization

The application uses a mobile-first responsive design approach without PWA features. Key optimizations include:

- **Touch-friendly controls**: Minimum 44px touch targets
- **Responsive grid layouts**: 1 column on mobile, 2-3 on desktop
- **Fluid typography**: Text scales based on screen size
- **Safe area support**: Handles notched devices (iPhone X+)
- **Optimized images**: Responsive album art sizing
- **Reduced animations on mobile**: Better performance on lower-end devices

**Responsive Layout Configuration**

```typescript
// tailwind.config.js
export default {
  theme: {
    extend: {
      screens: {
        'xs': '475px',
        // sm: '640px', (default)
        // md: '768px', (default)
        // lg: '1024px', (default)
        // xl: '1280px', (default)
        '2xl': '1536px',
      },
    },
  },
};
```

**Mobile-First Styles (app.css)**

```css
/* Base mobile styles */
:root {
  /* Touch-friendly sizing */
  --touch-target-min: 44px;

  /* Responsive spacing */
  --spacing-mobile: 1rem;
  --spacing-desktop: 2rem;
}

/* Prevent text size adjustment on mobile */
html {
  -webkit-text-size-adjust: 100%;
  -moz-text-size-adjust: 100%;
  text-size-adjust: 100%;
}

/* Touch-friendly buttons */
button, a {
  min-height: var(--touch-target-min);
  min-width: var(--touch-target-min);
  -webkit-tap-highlight-color: transparent;
}

/* Optimize for mobile scrolling */
body {
  overscroll-behavior-y: contain;
  -webkit-overflow-scrolling: touch;
}

/* Safe area support for notched devices */
@supports (padding: max(0px)) {
  body {
    padding-left: max(0px, env(safe-area-inset-left));
    padding-right: max(0px, env(safe-area-inset-right));
    padding-bottom: max(0px, env(safe-area-inset-bottom));
  }
}
```

**Viewport Meta Tags (app.html)**

```html
<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover" />
  <meta name="theme-color" content="#3b82f6" />
  <meta name="description" content="AI-powered radio stations from your Navidrome library" />

  <!-- Favicon -->
  <link rel="icon" href="/favicon.ico" />
  <link rel="apple-touch-icon" href="/icons/icon-192x192.png" />

  <!-- Disable zooming on input focus (optional) -->
  <meta name="viewport" content="width=device-width, initial-scale=1, maximum-scale=1, user-scalable=no" />

  %sveltekit.head%
</head>
<body data-sveltekit-preload-data="hover">
  <div style="display: contents">%sveltekit.body%</div>
</body>
</html>
```

### 5.4 Responsive Station List Component

```svelte
<script lang="ts">
  import { onMount } from 'svelte';
  import type { Station } from '$lib/types';

  let stations = $state<Station[]>([]);
  let loading = $state(true);

  onMount(async () => {
    const response = await fetch('/api/v1/stations');
    stations = await response.json();
    loading = false;
  });
</script>

<div class="container mx-auto px-4 py-6 md:py-12">
  <h1 class="text-3xl md:text-5xl font-bold mb-6 md:mb-10 text-center">
    Radio Stations
  </h1>

  {#if loading}
    <div class="flex items-center justify-center min-h-[50vh]">
      <div class="animate-spin rounded-full h-12 w-12 border-b-2 border-blue-600"></div>
    </div>
  {:else}
    <!-- Responsive grid: 1 column on mobile, 2 on tablet, 3 on desktop -->
    <div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4 md:gap-6">
      {#each stations as station}
        <a
          href="/{station.path}"
          class="station-card group block bg-gray-800 rounded-lg overflow-hidden shadow-lg hover:shadow-2xl transition-all duration-300 hover:scale-105 active:scale-100"
        >
          <!-- Visual indicator for active stations -->
          <div class="relative">
            <div class="aspect-square bg-gradient-to-br from-blue-600 to-purple-600 flex items-center justify-center">
              <svg class="w-16 h-16 md:w-20 md:h-20 text-white" fill="currentColor" viewBox="0 0 20 20">
                <path d="M18 3a1 1 0 00-1.196-.98l-10 2A1 1 0 006 5v9.114A4.369 4.369 0 005 14c-1.657 0-3 .895-3 2s1.343 2 3 2 3-.895 3-2V7.82l8-1.6v5.894A4.37 4.37 0 0015 12c-1.657 0-3 .895-3 2s1.343 2 3 2 3-.895 3-2V3z"/>
              </svg>
            </div>
            {#if station.active}
              <div class="absolute top-2 right-2 flex items-center gap-1 bg-green-500 text-white px-2 py-1 rounded-full text-xs md:text-sm font-semibold">
                <span class="w-2 h-2 bg-white rounded-full animate-pulse"></span>
                Live
              </div>
            {/if}
          </div>

          <!-- Station info -->
          <div class="p-4 md:p-6">
            <h3 class="text-lg md:text-xl font-bold mb-2 group-hover:text-blue-400 transition-colors truncate">
              {station.name}
            </h3>
            <p class="text-sm md:text-base text-gray-400 mb-3 line-clamp-2">
              {station.description}
            </p>

            <!-- Genre tags -->
            <div class="flex flex-wrap gap-1.5">
              {#each station.genres.slice(0, 3) as genre}
                <span class="px-2 py-1 bg-gray-700 rounded text-xs md:text-sm text-gray-300">
                  {genre}
                </span>
              {/each}
              {#if station.genres.length > 3}
                <span class="px-2 py-1 bg-gray-700 rounded text-xs md:text-sm text-gray-300">
                  +{station.genres.length - 3}
                </span>
              {/if}
            </div>
          </div>
        </a>
      {/each}
    </div>
  {/if}
</div>

<style>
  /* Line clamp for multiline truncation */
  .line-clamp-2 {
    display: -webkit-box;
    -webkit-line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }

  /* Touch optimization */
  .station-card {
    -webkit-tap-highlight-color: transparent;
    touch-action: manipulation;
  }
</style>
```

## 6. Database Schema

### 6.1 PostgreSQL Schema

```sql
-- Enable extensions
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";
CREATE EXTENSION IF NOT EXISTS "pgvector";

-- Users table
CREATE TABLE users (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    username VARCHAR(50) UNIQUE NOT NULL,
    email VARCHAR(255) UNIQUE NOT NULL,
    password_hash VARCHAR(255) NOT NULL,
    role VARCHAR(20) NOT NULL CHECK (role IN ('admin', 'listener')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_login TIMESTAMPTZ
);

CREATE INDEX idx_users_username ON users(username);
CREATE INDEX idx_users_email ON users(email);

-- Stations table
CREATE TABLE stations (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    path VARCHAR(100) UNIQUE NOT NULL,
    name VARCHAR(255) NOT NULL,
    description TEXT NOT NULL,
    genres TEXT[] NOT NULL,
    mood_tags TEXT[],
    created_by UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    active BOOLEAN NOT NULL DEFAULT false,
    config JSONB NOT NULL,
    embedding vector(3072)  -- Station description embedding
);

CREATE INDEX idx_stations_path ON stations(path);
CREATE INDEX idx_stations_active ON stations(active);
CREATE INDEX idx_stations_embedding ON stations USING ivfflat (embedding vector_cosine_ops);

-- Tracks table (cached from Navidrome)
CREATE TABLE tracks (
    id VARCHAR(100) PRIMARY KEY,  -- Navidrome track ID
    title VARCHAR(500) NOT NULL,
    artist VARCHAR(500) NOT NULL,
    album VARCHAR(500) NOT NULL,
    genre TEXT[] NOT NULL,
    year INTEGER,
    duration INTEGER NOT NULL,
    path TEXT NOT NULL,
    metadata JSONB,
    embedding vector(3072),  -- Track content embedding
    last_synced TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_tracks_genre ON tracks USING GIN(genre);
CREATE INDEX idx_tracks_artist ON tracks(artist);
CREATE INDEX idx_tracks_embedding ON tracks USING ivfflat (embedding vector_cosine_ops);

-- Playlist history (tracks played on each station)
CREATE TABLE playlist_history (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    station_id UUID NOT NULL REFERENCES stations(id) ON DELETE CASCADE,
    track_id VARCHAR(100) NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
    played_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    selection_method VARCHAR(50) NOT NULL,  -- 'ai_contextual', 'ai_embeddings', 'random'
    skipped BOOLEAN NOT NULL DEFAULT false
);

CREATE INDEX idx_playlist_history_station ON playlist_history(station_id, played_at DESC);
CREATE INDEX idx_playlist_history_track ON playlist_history(track_id);

-- Listener analytics
CREATE TABLE listener_sessions (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    station_id UUID NOT NULL REFERENCES stations(id) ON DELETE CASCADE,
    user_id UUID REFERENCES users(id) ON DELETE SET NULL,  -- NULL for anonymous
    ip_address INET,
    user_agent TEXT,
    started_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    ended_at TIMESTAMPTZ,
    duration_seconds INTEGER
);

CREATE INDEX idx_listener_sessions_station ON listener_sessions(station_id, started_at DESC);

-- Function to update updated_at timestamp
CREATE OR REPLACE FUNCTION update_updated_at_column()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ language 'plpgsql';

CREATE TRIGGER update_users_updated_at BEFORE UPDATE ON users
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_stations_updated_at BEFORE UPDATE ON stations
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();
```

### 6.2 Redis Data Structures

```
# Active station state
station:{station_id}:current_track = {
    "track_id": "abc123",
    "started_at": "2025-11-23T12:00:00Z",
    "duration": 245
}

# Station listener count
station:{station_id}:listeners = 42  (Redis INCR/DECR)

# Station playlist queue
station:{station_id}:queue = ["track1", "track2", "track3"]  (Redis LIST)

# Recent tracks (to avoid repetition)
station:{station_id}:recent = ["track1", "track2", ...]  (Redis LIST, max 50)

# Session tokens
session:{token_hash} = {
    "user_id": "uuid",
    "role": "admin",
    "expires_at": "2025-11-30T12:00:00Z"
}  (TTL: 7 days)

# Rate limiting (per IP)
ratelimit:api:{ip}:{endpoint} = 10  (TTL: 60s, max requests per minute)
```

## 7. API Specification

### 7.1 RESTful API Endpoints

**Base URL**: `https://yourdomain.com/api/v1`

#### Authentication

```
POST /auth/register
POST /auth/login
POST /auth/logout
POST /auth/refresh
GET  /auth/me
```

#### Stations

```
GET    /stations                      # List all stations
POST   /stations                      # Create station (admin)
GET    /stations/:id                  # Get station details
PATCH  /stations/:id                  # Update station (admin)
DELETE /stations/:id                  # Delete station (admin)
POST   /stations/:id/start            # Start broadcasting (admin)
POST   /stations/:id/stop             # Stop broadcasting (admin)
POST   /stations/:id/skip             # Skip current track (admin)
GET    /stations/:id/nowplaying       # Current track + metadata
GET    /stations/:id/history          # Recently played tracks
GET    /stations/:id/analytics        # Listener stats (admin)
```

#### Streaming

```
GET /stream/:path/master.m3u8         # HLS master playlist
GET /stream/:path/playlist.m3u8       # HLS media playlist
GET /stream/:path/segment/:num.ts     # HLS segment
```

#### Library

```
GET /library/genres                   # Available genres from Navidrome
GET /library/search?q=...             # Search tracks (admin)
```

### 7.2 WebSocket API

**Connection**: `wss://yourdomain.com/ws/stations/:path`

#### Client → Server Messages

```typescript
interface ClientMessage {
  type: 'ping' | 'auth';
  token?: string;  // For authenticated sessions
}
```

#### Server → Client Messages

```typescript
interface ServerMessage {
  type: 'nowPlaying' | 'trackChanged' | 'listeners' | 'stationStopped';
  data: any;
}

interface NowPlayingMessage {
  type: 'nowPlaying';
  data: {
    track: {
      id: string;
      title: string;
      artist: string;
      album: string;
      albumArt: string;
      duration: number;
    };
    startedAt: string;  // ISO 8601
    listeners: number;
  };
}

interface TrackChangedMessage {
  type: 'trackChanged';
  data: {
    track: Track;
    startedAt: string;
  };
}
```

### 7.3 Example API Payloads

**Create Station**

```http
POST /api/v1/stations
Authorization: Bearer {token}
Content-Type: application/json

{
  "path": "indie-vibes",
  "name": "Indie Vibes",
  "description": "Chill indie music with dreamy vocals and mellow instrumentals. Perfect for working or relaxing. Think Bon Iver, Phoebe Bridgers, and The National.",
  "genres": ["Indie Rock", "Indie Pop", "Alternative"],
  "mood_tags": ["chill", "dreamy", "mellow", "introspective"],
  "config": {
    "bitrate": 192,
    "sample_rate": 44100,
    "crossfade_ms": 3000,
    "track_selection_mode": "AIContextual",
    "min_track_duration": 120,
    "max_track_duration": 420,
    "explicit_content": false
  }
}
```

**Response**

```json
{
  "id": "550e8400-e29b-41d4-a716-446655440000",
  "path": "indie-vibes",
  "name": "Indie Vibes",
  "description": "Chill indie music...",
  "genres": ["Indie Rock", "Indie Pop", "Alternative"],
  "mood_tags": ["chill", "dreamy", "mellow", "introspective"],
  "created_by": "user-id",
  "created_at": "2025-11-23T12:00:00Z",
  "updated_at": "2025-11-23T12:00:00Z",
  "active": false,
  "config": { /* ... */ }
}
```

## 8. Performance Requirements

### 8.1 Latency Targets

- **API Response Time**: p50 < 50ms, p99 < 200ms
- **Stream Startup**: < 2 seconds from page load to first audio
- **Synchronization Drift**: < 500ms between listeners
- **Track Transition**: < 100ms gap between tracks
- **Admin Skip**: < 1 second for all listeners to receive new track

### 8.2 Throughput Targets

- **Concurrent Listeners**: 10,000+ per instance
- **Concurrent Stations**: 100+ active stations per instance
- **API Requests**: 10,000 requests/second per instance
- **WebSocket Connections**: 10,000+ concurrent connections

### 8.3 Resource Limits

- **Memory per Station**: < 50MB for encoder + buffers
- **CPU per Station**: < 10% of single core (at 256kbps)
- **Disk Cache**: 10GB max for segment storage (FIFO eviction)
- **Database Connections**: Pool of 20-50 connections

### 8.4 Optimization Strategies

#### Caching

```rust
// Three-tier caching strategy
pub struct CacheHierarchy {
    // L1: In-memory LRU (hot segments, ~100MB)
    memory: Arc<Mutex<LruCache<SegmentKey, Bytes>>>,

    // L2: Redis (warm segments, ~1GB)
    redis: Arc<RedisClient>,

    // L3: Disk (cold segments, ~10GB)
    disk: Arc<DiskCache>,
}

impl CacheHierarchy {
    async fn get(&self, key: &SegmentKey) -> Option<Bytes> {
        // Try L1
        if let Some(data) = self.memory.lock().await.get(key) {
            return Some(data.clone());
        }

        // Try L2
        if let Some(data) = self.redis.get(key).await.ok()? {
            // Promote to L1
            self.memory.lock().await.put(key.clone(), data.clone());
            return Some(data);
        }

        // Try L3
        if let Some(data) = self.disk.get(key).await.ok()? {
            // Promote to L1 and L2
            self.memory.lock().await.put(key.clone(), data.clone());
            self.redis.set(key, &data, 300).await.ok();
            return Some(data);
        }

        None
    }
}
```

#### Connection Pooling

```rust
// Database connection pool
let db_pool = sqlx::postgres::PgPoolOptions::new()
    .max_connections(50)
    .min_connections(10)
    .acquire_timeout(Duration::from_secs(3))
    .idle_timeout(Duration::from_secs(600))
    .max_lifetime(Duration::from_secs(1800))
    .connect(&database_url)
    .await?;

// HTTP client with keep-alive
let http_client = reqwest::Client::builder()
    .pool_max_idle_per_host(20)
    .pool_idle_timeout(Duration::from_secs(90))
    .timeout(Duration::from_secs(30))
    .build()?;
```

#### Zero-Copy Operations

```rust
// Use bytes::Bytes for zero-copy segment sharing
async fn serve_segment(
    segment_cache: Arc<SegmentCache>,
    key: SegmentKey
) -> Result<Response<Body>, Error> {
    let data = segment_cache.get(&key).await?;

    // Bytes can be cheaply cloned (refcounted)
    Ok(Response::builder()
        .header("Content-Type", "video/mp2t")
        .header("Cache-Control", "max-age=3600")
        .body(Body::from(data))
        .unwrap())
}
```

## 9. Security Considerations

### 9.1 Authentication Security

- **Password Hashing**: Argon2id with 19MiB memory, 2 iterations, 1 parallelism
- **JWT Tokens**: HS256 with 256-bit secret, 7-day expiry
- **Token Storage**: HttpOnly cookies + Authorization header support
- **CSRF Protection**: SameSite=Strict cookies + CSRF tokens for state-changing ops

### 9.2 Authorization Model

```rust
// Role-based access control
#[derive(Debug)]
struct Permission {
    resource: Resource,
    action: Action,
}

enum Resource {
    Station,
    Track,
    User,
    Analytics,
}

enum Action {
    Read,
    Create,
    Update,
    Delete,
    Skip,
}

impl User {
    fn can(&self, permission: Permission) -> bool {
        match (self.role, permission) {
            // Admins can do everything
            (UserRole::Admin, _) => true,

            // Listeners can only read
            (UserRole::Listener, Permission { action: Action::Read, .. }) => true,

            // Deny by default
            _ => false,
        }
    }
}
```

### 9.3 Rate Limiting

```rust
use governor::{Quota, RateLimiter};

// Per-IP rate limiting
let limiter = RateLimiter::keyed(
    Quota::per_minute(nonzero!(60u32))  // 60 requests/min
);

async fn rate_limit_middleware(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    match limiter.check_key(&addr.ip()) {
        Ok(_) => Ok(next.run(request).await),
        Err(_) => Err(StatusCode::TOO_MANY_REQUESTS),
    }
}
```

### 9.4 Input Validation

```rust
use validator::{Validate, ValidationError};

#[derive(Debug, Validate)]
struct CreateStationRequest {
    #[validate(length(min = 1, max = 100), regex = "^[a-z0-9-]+$")]
    path: String,

    #[validate(length(min = 1, max = 255))]
    name: String,

    #[validate(length(min = 10, max = 2000))]
    description: String,

    #[validate(length(min = 1, max = 20))]
    genres: Vec<String>,

    #[validate]
    config: StationConfig,
}

impl CreateStationRequest {
    async fn validate_unique_path(&self, db: &PgPool) -> Result<(), ValidationError> {
        let exists = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM stations WHERE path = $1)"
        )
        .bind(&self.path)
        .fetch_one(db)
        .await
        .map_err(|_| ValidationError::new("database_error"))?;

        if exists {
            return Err(ValidationError::new("path_taken"));
        }

        Ok(())
    }
}
```

### 9.5 Content Security

- **CORS**: Strict origin validation, credentials required
- **HTTPS Only**: HSTS with max-age=31536000, includeSubDomains
- **Content-Type**: Strict validation on file uploads
- **SQL Injection**: Parameterized queries only (sqlx compile-time verification)
- **XSS Prevention**: CSP headers, sanitized output

```rust
// Content Security Policy
fn security_headers() -> HeaderMap {
    let mut headers = HeaderMap::new();

    headers.insert(
        "Content-Security-Policy",
        "default-src 'self'; \
         script-src 'self' 'unsafe-inline'; \
         style-src 'self' 'unsafe-inline'; \
         media-src 'self' blob:; \
         connect-src 'self' wss://yourdomain.com".parse().unwrap()
    );

    headers.insert(
        "X-Frame-Options",
        "DENY".parse().unwrap()
    );

    headers.insert(
        "X-Content-Type-Options",
        "nosniff".parse().unwrap()
    );

    headers.insert(
        "Referrer-Policy",
        "strict-origin-when-cross-origin".parse().unwrap()
    );

    headers
}
```

## 10. Deployment Architecture

### 10.1 Docker Composition

**docker-compose.yml**

```yaml
version: '3.9'

services:
  # Backend API + Streaming
  backend:
    build:
      context: .
      dockerfile: Dockerfile
      target: production
    restart: unless-stopped
    environment:
      DATABASE_URL: postgres://user:pass@postgres:5432/navidrome_radio
      REDIS_URL: redis://redis:6379
      NAVIDROME_URL: http://navidrome:4533
      NAVIDROME_USER: admin
      NAVIDROME_PASS: ${NAVIDROME_PASSWORD}
      ANTHROPIC_API_KEY: ${ANTHROPIC_API_KEY}
      OPENAI_API_KEY: ${OPENAI_API_KEY}
      JWT_SECRET: ${JWT_SECRET}
      RUST_LOG: info
    volumes:
      - segment_cache:/var/cache/segments
    depends_on:
      - postgres
      - redis
    networks:
      - app_network

  # PostgreSQL with pgvector
  postgres:
    image: pgvector/pgvector:pg16
    restart: unless-stopped
    environment:
      POSTGRES_DB: navidrome_radio
      POSTGRES_USER: user
      POSTGRES_PASSWORD: ${POSTGRES_PASSWORD}
    volumes:
      - postgres_data:/var/lib/postgresql/data
    networks:
      - app_network

  # Redis
  redis:
    image: redis:7-alpine
    restart: unless-stopped
    command: redis-server --maxmemory 1gb --maxmemory-policy allkeys-lru
    volumes:
      - redis_data:/data
    networks:
      - app_network

  # Navidrome (assumed external, but can be included)
  navidrome:
    image: deluan/navidrome:latest
    restart: unless-stopped
    environment:
      ND_SCANSCHEDULE: 1h
      ND_LOGLEVEL: info
    volumes:
      - navidrome_data:/data
      - ${MUSIC_DIR}:/music:ro
    networks:
      - app_network

  # Caddy reverse proxy
  caddy:
    image: caddy:2-alpine
    restart: unless-stopped
    ports:
      - "80:80"
      - "443:443"
    volumes:
      - ./Caddyfile:/etc/caddy/Caddyfile
      - caddy_data:/data
      - caddy_config:/config
    networks:
      - app_network
    depends_on:
      - backend

volumes:
  postgres_data:
  redis_data:
  navidrome_data:
  segment_cache:
  caddy_data:
  caddy_config:

networks:
  app_network:
    driver: bridge
```

**Caddyfile**

```
yourdomain.com {
    # Frontend (SvelteKit)
    handle /assets/* {
        root * /var/www/frontend
        file_server
        header Cache-Control "public, max-age=31536000, immutable"
    }

    # API
    handle /api/* {
        reverse_proxy backend:8000
    }

    # WebSocket
    handle /ws/* {
        reverse_proxy backend:8000
    }

    # HLS streams
    handle /stream/* {
        reverse_proxy backend:8000
        header Cache-Control "public, max-age=3600"
    }

    # SPA fallback
    handle {
        root * /var/www/frontend
        try_files {path} /index.html
        file_server
    }

    # Security headers
    header {
        Strict-Transport-Security "max-age=31536000; includeSubDomains"
        X-Content-Type-Options "nosniff"
        X-Frame-Options "DENY"
        Referrer-Policy "strict-origin-when-cross-origin"
    }

    # Logging
    log {
        output file /var/log/caddy/access.log
    }
}
```

### 10.2 Multi-Stage Dockerfile

```dockerfile
# Build stage
FROM rust:1.75-slim as builder

WORKDIR /app

# Install dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    libpq-dev \
    libavcodec-dev \
    libavformat-dev \
    libavutil-dev \
    && rm -rf /var/lib/apt/lists/*

# Cache dependencies
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN cargo build --release && rm -rf src

# Build application
COPY src ./src
RUN touch src/main.rs && cargo build --release

# Runtime stage
FROM debian:bookworm-slim as production

RUN apt-get update && apt-get install -y \
    ca-certificates \
    libpq5 \
    libavcodec59 \
    libavformat59 \
    libavutil57 \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY --from=builder /app/target/release/navidrome-radio .

# Create cache directory
RUN mkdir -p /var/cache/segments

EXPOSE 8000

CMD ["./navidrome-radio"]
```

### 10.3 Kubernetes Deployment (Production)

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: navidrome-radio-backend
spec:
  replicas: 3
  selector:
    matchLabels:
      app: navidrome-radio
      component: backend
  template:
    metadata:
      labels:
        app: navidrome-radio
        component: backend
    spec:
      containers:
      - name: backend
        image: yourdomain.com/navidrome-radio:latest
        ports:
        - containerPort: 8000
        env:
        - name: DATABASE_URL
          valueFrom:
            secretKeyRef:
              name: database-credentials
              key: url
        - name: REDIS_URL
          value: redis://redis-service:6379
        resources:
          requests:
            memory: "512Mi"
            cpu: "500m"
          limits:
            memory: "2Gi"
            cpu: "2000m"
        livenessProbe:
          httpGet:
            path: /health
            port: 8000
          initialDelaySeconds: 30
          periodSeconds: 10
        readinessProbe:
          httpGet:
            path: /ready
            port: 8000
          initialDelaySeconds: 5
          periodSeconds: 5
---
apiVersion: v1
kind: Service
metadata:
  name: backend-service
spec:
  selector:
    app: navidrome-radio
    component: backend
  ports:
  - port: 8000
    targetPort: 8000
  type: LoadBalancer
```

## 11. Monitoring & Observability

### 11.1 Metrics (Prometheus)

```rust
use prometheus::{Counter, Histogram, Gauge, Registry};

lazy_static! {
    static ref REGISTRY: Registry = Registry::new();

    static ref HTTP_REQUESTS: Counter = Counter::new(
        "http_requests_total",
        "Total HTTP requests"
    ).unwrap();

    static ref HTTP_REQUEST_DURATION: Histogram = Histogram::with_opts(
        HistogramOpts::new(
            "http_request_duration_seconds",
            "HTTP request duration"
        ).buckets(vec![0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5])
    ).unwrap();

    static ref ACTIVE_LISTENERS: Gauge = Gauge::new(
        "active_listeners_total",
        "Current number of active listeners"
    ).unwrap();

    static ref ACTIVE_STATIONS: Gauge = Gauge::new(
        "active_stations_total",
        "Current number of broadcasting stations"
    ).unwrap();

    static ref TRACKS_PLAYED: Counter = Counter::new(
        "tracks_played_total",
        "Total tracks played across all stations"
    ).unwrap();

    static ref SEGMENTS_CACHED: Gauge = Gauge::new(
        "segments_cached_total",
        "Number of segments in cache"
    ).unwrap();
}
```

### 11.2 Logging (Structured)

```rust
use tracing::{info, warn, error, instrument};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

fn init_logging() {
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "info".into())
        ))
        .with(tracing_subscriber::fmt::layer().json())
        .init();
}

#[instrument(skip(db))]
async fn create_station(
    db: &PgPool,
    request: CreateStationRequest
) -> Result<Station, Error> {
    info!(
        station_path = %request.path,
        genres = ?request.genres,
        "Creating new station"
    );

    // ... implementation

    info!(
        station_id = %station.id,
        "Station created successfully"
    );

    Ok(station)
}
```

### 11.3 Dashboards (Grafana)

**Key Metrics Dashboard**:
- Active listeners (by station)
- Stream startup time (p50, p95, p99)
- API latency (by endpoint)
- Error rate (by type)
- Cache hit rate (L1, L2, L3)
- Database connection pool usage
- Memory usage per station
- CPU usage per station

## 12. Testing Strategy

### 12.1 Unit Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_station_creation() {
        let db = setup_test_db().await;

        let request = CreateStationRequest {
            path: "test-station".to_string(),
            name: "Test Station".to_string(),
            description: "Test description".to_string(),
            genres: vec!["Rock".to_string()],
            mood_tags: vec![],
            config: StationConfig::default(),
        };

        let station = create_station(&db, request).await.unwrap();

        assert_eq!(station.path, "test-station");
        assert!(!station.active);
    }

    #[test]
    fn test_segment_number_calculation() {
        let start = Utc.with_ymd_and_hms(2025, 11, 23, 12, 0, 0).unwrap();
        let now = Utc.with_ymd_and_hms(2025, 11, 23, 12, 0, 10).unwrap();

        let segment = calculate_segment_number(start, now, 2.0);

        assert_eq!(segment, 5); // 10 seconds / 2 seconds per segment
    }
}
```

### 12.2 Integration Tests

```rust
#[tokio::test]
async fn test_end_to_end_station_workflow() {
    let app = setup_test_app().await;

    // 1. Create station
    let create_response = app
        .post("/api/v1/stations")
        .json(&CreateStationRequest { /* ... */ })
        .send()
        .await;

    assert_eq!(create_response.status(), 201);
    let station: Station = create_response.json().await;

    // 2. Start broadcasting
    let start_response = app
        .post(&format!("/api/v1/stations/{}/start", station.id))
        .send()
        .await;

    assert_eq!(start_response.status(), 200);

    // 3. Verify HLS manifest is available
    let manifest_response = app
        .get(&format!("/stream/{}/master.m3u8", station.path))
        .send()
        .await;

    assert_eq!(manifest_response.status(), 200);
    assert!(manifest_response.text().await.contains("#EXTM3U"));
}
```

### 12.3 Load Tests (k6)

```javascript
import http from 'k6/http';
import { check, sleep } from 'k6';

export const options = {
  stages: [
    { duration: '2m', target: 100 },   // Ramp up to 100 listeners
    { duration: '5m', target: 100 },   // Stay at 100 for 5 minutes
    { duration: '2m', target: 1000 },  // Ramp up to 1000
    { duration: '5m', target: 1000 },  // Stay at 1000 for 5 minutes
    { duration: '2m', target: 0 },     // Ramp down
  ],
};

export default function () {
  // Simulate listener joining station
  const manifest = http.get('https://yourdomain.com/stream/rock/master.m3u8');
  check(manifest, {
    'manifest is status 200': (r) => r.status === 200,
    'manifest contains m3u8': (r) => r.body.includes('#EXTM3U'),
  });

  // Fetch playlist
  const playlist = http.get('https://yourdomain.com/stream/rock/playlist.m3u8');
  check(playlist, {
    'playlist is status 200': (r) => r.status === 200,
  });

  // Simulate listening for 30 seconds (fetch segments)
  for (let i = 0; i < 15; i++) {
    http.get(`https://yourdomain.com/stream/rock/segment/${i}.ts`);
    sleep(2);
  }
}
```

## 13. Development Workflow

### 13.1 Project Structure

```
navidrome-radio/
├── Cargo.toml
├── Cargo.lock
├── rust-toolchain.toml
├── .env.example
├── docker-compose.yml
├── Dockerfile
├── Caddyfile
├── README.md
├── SPECIFICATION.md           # This document
├── backend/
│   ├── src/
│   │   ├── main.rs
│   │   ├── lib.rs
│   │   ├── api/
│   │   │   ├── mod.rs
│   │   │   ├── auth.rs
│   │   │   ├── stations.rs
│   │   │   └── streaming.rs
│   │   ├── services/
│   │   │   ├── mod.rs
│   │   │   ├── station_manager.rs
│   │   │   ├── streaming_engine.rs
│   │   │   ├── ai_curation.rs
│   │   │   └── navidrome_client.rs
│   │   ├── models/
│   │   │   ├── mod.rs
│   │   │   ├── user.rs
│   │   │   ├── station.rs
│   │   │   └── track.rs
│   │   ├── db/
│   │   │   ├── mod.rs
│   │   │   └── migrations/
│   │   ├── config.rs
│   │   └── error.rs
│   └── tests/
│       ├── integration/
│       └── fixtures/
├── frontend/
│   ├── package.json
│   ├── vite.config.ts
│   ├── svelte.config.js
│   ├── tsconfig.json
│   ├── src/
│   │   ├── routes/
│   │   ├── lib/
│   │   └── app.html
│   └── static/
│       ├── favicon.ico
│       └── icons/
├── k6/
│   └── load-test.js
└── .github/
    └── workflows/
        ├── ci.yml
        └── deploy.yml
```

### 13.2 Development Commands

```bash
# Setup
cp .env.example .env
docker-compose up -d postgres redis navidrome

# Backend development
cd backend
cargo watch -x run

# Frontend development
cd frontend
npm install
npm run dev

# Database migrations
sqlx migrate run

# Tests
cargo test
npm test

# Load testing
k6 run k6/load-test.js

# Production build
docker-compose build
docker-compose up -d
```

### 13.3 CI/CD Pipeline (.github/workflows/ci.yml)

```yaml
name: CI/CD

on:
  push:
    branches: [main]
  pull_request:
    branches: [main]

jobs:
  test-backend:
    runs-on: ubuntu-latest
    services:
      postgres:
        image: pgvector/pgvector:pg16
        env:
          POSTGRES_PASSWORD: postgres
        options: >-
          --health-cmd pg_isready
          --health-interval 10s
          --health-timeout 5s
          --health-retries 5
      redis:
        image: redis:7-alpine
        options: >-
          --health-cmd "redis-cli ping"
          --health-interval 10s
          --health-timeout 5s
          --health-retries 5
    steps:
      - uses: actions/checkout@v4
      - uses: actions-rs/toolchain@v1
        with:
          toolchain: stable
      - uses: Swatinem/rust-cache@v2
      - name: Run tests
        run: cargo test --all-features
      - name: Check formatting
        run: cargo fmt -- --check
      - name: Clippy
        run: cargo clippy -- -D warnings

  test-frontend:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v4
        with:
          node-version: '20'
      - name: Install dependencies
        run: cd frontend && npm ci
      - name: Run tests
        run: cd frontend && npm test
      - name: Build
        run: cd frontend && npm run build

  deploy:
    needs: [test-backend, test-frontend]
    if: github.ref == 'refs/heads/main'
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Build and push Docker image
        run: |
          docker build -t yourdomain.com/navidrome-radio:latest .
          docker push yourdomain.com/navidrome-radio:latest
      - name: Deploy to production
        run: |
          # Deploy commands (kubectl, helm, etc.)
```

## 14. Future Enhancements

### 14.1 Phase 2 Features

1. **Custom Playlists**: Allow admins to queue specific tracks
2. **Scheduled Programming**: Time-based station configurations
3. **User Requests**: Listener song requests (with voting)
4. **Social Features**: Chat, reactions, listener profiles
5. **Analytics Dashboard**: Detailed listener insights
6. **Multi-Language Support**: i18n for global audience
7. **Podcast Support**: Mix podcasts into music rotation
8. **Smart Crossfading**: Beat-matched transitions
9. **Dark/Light Theme Toggle**: User preference for UI theme
10. **Federation**: Connect multiple Navidrome instances
11. **Offline Station List**: Cache station metadata for faster loading
12. **Share Station Links**: Deep linking to specific stations

### 14.2 Advanced AI Features

1. **Mood Detection**: Analyze audio features to match mood
2. **Collaborative Filtering**: Learn from listener behavior
3. **Dynamic Descriptions**: LLM-generated station descriptions
4. **Voice Commands**: "Play something upbeat"
5. **Smart Scheduling**: Auto-adjust playlist to time of day

## 15. Appendices

### 15.1 Glossary

- **HLS**: HTTP Live Streaming, Apple's adaptive bitrate streaming protocol
- **LLM**: Large Language Model
- **PWA**: Progressive Web App
- **Navidrome**: Open-source music server compatible with Subsonic API
- **Segment**: Small chunk of audio (typically 2-10 seconds)
- **Manifest**: Playlist file listing available segments

### 15.2 References

- [HLS Specification](https://datatracker.ietf.org/doc/html/rfc8216)
- [Navidrome API Documentation](https://www.navidrome.org/docs/developers/subsonic-api/)
- [Anthropic Claude API](https://docs.anthropic.com/)
- [Rust Axum Framework](https://github.com/tokio-rs/axum)
- [SvelteKit Documentation](https://kit.svelte.dev/)
- [Tailwind CSS Responsive Design](https://tailwindcss.com/docs/responsive-design)
- [Web.dev Mobile Performance](https://web.dev/explore/progressive-web-apps)

### 15.3 License

This specification is provided as a design document. Implementation will require appropriate licensing for dependencies and API services.

---

**Document Version**: 1.0.0
**Last Updated**: 2025-11-23
**Authors**: Technical Architecture Team
**Status**: Ready for Implementation
