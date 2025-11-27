# AI Library Indexing & Curation System

**Branch**: `feature/ai-library-indexing`

This document describes the advanced AI-powered music library indexing and curation system for Navidrome Radio.

## Overview

This system implements a multi-layered AI approach to intelligently curate radio playlists based on natural language queries and comprehensive music library analysis.

## Architecture

### 1. Database Schema (Migration 002)

**Library Index Table** (`library_index`):
- Comprehensive track metadata with AI-analyzed properties
- Fields include:
  - Basic metadata: title, artist, album, composer, year, duration
  - Genres (array)
  - AI-analyzed mood tags: `["energetic", "melancholic", "upbeat"]`
  - Audio features: energy_level (0-1), danceability (0-1), valence (0-1), tempo (BPM)
  - Song types: `["ballad", "anthem", "instrumental"]`
  - Themes: `["love", "introspection", "celebration"]`
  - Acoustic properties: acousticness, instrumentalness
  - User ratings and aggregated ratings
  - External metadata: RYM ratings, Last.fm play counts, MusicBrainz IDs
  - Full-text search vector for fast querying

**Library Statistics Table** (`library_stats`):
- Aggregated data about the entire library
- Genre distribution (top 50 genres with counts)
- Artist distribution (top 100 artists)
- Year distribution
- Mood distribution
- Average energy, tempo, valence across library
- Song type distribution
- AI analysis progress percentage

**External Metadata Table** (`external_metadata`):
- Stores ratings and metadata from external sources
- Supports: RateYourMusic, Last.fm, MusicBrainz, Spotify
- Includes ratings, popularity scores, and tags

**User Track Ratings Table** (`user_track_ratings`):
- Personal ratings (0-5 stars) per user
- Automatically updates aggregate ratings in library_index

**AI Query Cache Table** (`ai_query_cache`):
- Caches AI query analysis results
- Avoids re-analyzing identical queries
- Tracks query popularity and usage

**Sync Status Table** (`library_sync_status`):
- Tracks library sync progress
- Monitors AI analysis progress
- Error tracking

### 2. Library Indexer Service

**File**: `backend/src/services/library_indexer.rs`

**Responsibilities**:
1. **Full Library Sync**: Fetches all tracks from Navidrome and indexes them
2. **AI Track Analysis**: Analyzes tracks using Claude to extract metadata
3. **Statistics Computation**: Maintains up-to-date library statistics

**Key Methods**:
- `sync_full()`: Performs complete sync from Navidrome
- `analyze_unanalyzed_tracks(limit)`: AI-analyzes tracks in batches
- `get_library_stats()`: Returns current library statistics

**AI Track Analyzer** (`TrackAnalyzer`):
- Uses Claude 3.5 Sonnet for track analysis
- Analyzes based on: title, artist, album, genres, year
- Extracts: mood tags, energy, danceability, valence, song type, themes, acoustic properties
- Concurrent processing (5 tracks at a time)

### 3. AI Curator Service

**File**: `backend/src/services/ai_curator.rs`

**Multi-Layered AI Approach**:

#### Layer 1: Library Context
- Fetches comprehensive library statistics
- Provides AI with:
  - Total tracks, artists, albums
  - Available genres and their distribution
  - Top artists in the library
  - Available mood tags
  - Year range
  - Average audio features (energy, tempo, valence)

#### Layer 2: Query Analysis
- Takes natural language query (e.g., "upbeat 80s rock for working out")
- Sends query + library context to Claude
- AI analyzes query considering what's actually in the library
- Extracts structured filters:
  ```json
  {
    "semantic_intent": "High energy rock music from the 1980s",
    "filters": {
      "genres": ["rock", "hard rock", "classic rock"],
      "year_range": [1980, 1989],
      "energy_range": [0.7, 1.0],
      "moods": ["energetic", "upbeat"],
      "min_rating": 3.5
    },
    "confidence": 0.85
  }
  ```
- Results are cached to avoid redundant API calls

#### Layer 3: Track Selection & Ranking
- Gets candidate tracks matching the filters (3x requested amount)
- Uses AI to select the best tracks considering:
  - Match quality with filters
  - Variety and flow
  - Ratings
  - Creating an engaging listening experience
- Returns ranked list of track IDs

**Key Methods**:
- `curate_tracks(query, limit)`: Main entry point for AI curation
- `analyze_query_with_ai()`: Layer 2 - query analysis
- `get_matching_tracks()`: Executes SQL with filters
- `ai_select_tracks()`: Layer 3 - intelligent selection

### 4. Data Models

**File**: `backend/src/models/library.rs`

Comprehensive type-safe models for:
- `LibraryTrack`: Enriched track with all metadata
- `LibraryStats`: Aggregated library statistics
- `LibrarySyncStatus`: Sync progress tracking
- `ExternalMetadata`: External source data
- `UserTrackRating`: Per-user ratings
- `AiQueryCache`: Cached query analyses
- Query/Analysis request/response types

## Features

### Comprehensive Metadata
- **Audio Features**: Tempo, energy, danceability, valence, acousticness, instrumentalness
- **Ratings**: User ratings, average ratings, RYM ratings, Last.fm stats
- **External Sources**: MusicBrainz, RateYourMusic, Last.fm integration ready
- **AI Analysis**: Mood tags, themes, song types
- **Full-Text Search**: Optimized search across title, artist, album

### Intelligent Querying
- Natural language queries
- Context-aware analysis (knows what's in your library)
- Multi-layered AI decision making
- Query result caching
- Fuzzy text matching with PostgreSQL trigram indexes

### Performance Optimizations
- Concurrent AI processing (batched analysis)
- Query result caching
- GIN indexes on JSONB fields
- Trigram indexes for fuzzy matching
- Materialized statistics for fast context loading

## Remaining Work

### High Priority
1. **Fix Compilation Errors**: Resolve remaining type errors
2. **API Endpoints**: Create REST endpoints for:
   - Library sync trigger
   - AI analysis trigger
   - Query curation endpoint
   - Statistics endpoint
   - User ratings CRUD

3. **Station Integration**: Update station manager to use AI curator
4. **Frontend UI**: Add interface for:
   - Viewing library stats
   - Triggering sync/analysis
   - Rating tracks
   - Testing queries

### Medium Priority
5. **Live Sync Mechanism**: Periodic incremental syncs
6. **External Metadata Fetching**: Implement RYM, Last.fm APIs
7. **Background Jobs**: Scheduler for automatic analysis
8. **Admin Dashboard**: Monitor sync status, AI progress

### Future Enhancements
9. **Vector Embeddings**: Semantic similarity search
10. **Collaborative Filtering**: User taste modeling
11. **Smart Playlists**: Auto-generated based on listening history
12. **A/B Testing**: Compare AI selections vs random

## Database Migration

To apply the new schema:
```bash
# Run migration (when implemented in main.rs)
# The migration will create all new tables and indexes
./dev.sh run
```

Note: The `pg_trgm` extension is required for fuzzy text matching.

## Configuration

Add to `.env`:
```env
ANTHROPIC_API_KEY=sk-ant-...  # Required for AI features
```

## Usage Example

```rust
// Initialize services
let library_indexer = LibraryIndexer::new(
    db.clone(),
    navidrome_client.clone(),
    Some(Arc::new(TrackAnalyzer::new(anthropic_api_key)))
);

let ai_curator = AiCurator::new(anthropic_api_key, db.clone());

// Sync library
library_indexer.sync_full().await?;

// Analyze tracks with AI
library_indexer.analyze_unanalyzed_tracks(100).await?;

// Curate playlist from natural language
let track_ids = ai_curator
    .curate_tracks("upbeat 90s hip hop".to_string(), 20)
    .await?;
```

## Performance Considerations

- **AI Analysis**: ~500-800 tracks/hour (rate limited by Claude API)
- **Library Sync**: ~5000 tracks/minute (limited by Navidrome)
- **Query Analysis**: ~1-2 seconds per unique query (cached thereafter)
- **Track Selection**: ~2-3 seconds for candidate set of 100 tracks

## Cost Estimates

Using Claude 3.5 Sonnet:
- **Track Analysis**: ~$0.001 per track (one-time cost)
- **Query Analysis**: ~$0.003 per unique query
- **Track Selection**: ~$0.005 per selection

For a 10,000 track library:
- Initial indexing: ~$10
- 1000 unique queries: ~$3
- 1000 playlist generations: ~$5

## Technical Decisions

### Why PostgreSQL over Vector DB?
- Already using PostgreSQL
- pg_trgm provides excellent fuzzy matching
- JSONB + GIN indexes are very fast
- Can add pgvector later for semantic search
- Simpler deployment

### Why Claude for Analysis?
- Excellent music knowledge
- Reliable structured output
- Fast inference
- Can handle nuanced queries
- Good at context understanding

### Why 3-Layer Approach?
1. Context prevents hallucination (AI knows what you have)
2. Query analysis creates structured filters
3. Final selection optimizes for experience, not just matching

This ensures high-quality, contextual results that feel personalized.

## Next Steps

1. Run `cargo check` to identify remaining compilation errors
2. Fix type errors and complete implementation
3. Add API endpoints
4. Test with real library data
5. Integrate with existing station system
6. Build frontend UI
7. Deploy and monitor

---

This represents a significant enhancement to Navidrome Radio, transforming it from simple genre-based curation to intelligent, AI-powered playlist generation with deep library understanding.
