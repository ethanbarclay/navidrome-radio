-- AI-Enhanced Library Index
-- This migration adds comprehensive music library indexing with AI-analyzed metadata

-- Enable pg_trgm extension for fuzzy text matching
CREATE EXTENSION IF NOT EXISTS pg_trgm;

-- Library index table with enriched metadata
CREATE TABLE library_index (
    id VARCHAR(100) PRIMARY KEY,  -- Same as track ID from Navidrome

    -- Basic metadata (from Navidrome)
    title VARCHAR(500) NOT NULL,
    artist VARCHAR(500) NOT NULL,
    album VARCHAR(500) NOT NULL,
    album_artist VARCHAR(500),
    composer VARCHAR(500),
    year INTEGER,
    duration INTEGER NOT NULL,

    -- Genre and categorization
    genres JSONB NOT NULL DEFAULT '[]'::jsonb,

    -- AI-analyzed metadata
    mood_tags JSONB NOT NULL DEFAULT '[]'::jsonb,  -- e.g., ["energetic", "melancholic", "upbeat"]
    energy_level FLOAT,  -- 0.0 to 1.0
    danceability FLOAT,  -- 0.0 to 1.0
    valence FLOAT,  -- 0.0 (sad) to 1.0 (happy)
    tempo FLOAT,  -- BPM

    -- Categorization
    song_type JSONB NOT NULL DEFAULT '[]'::jsonb,  -- e.g., ["ballad", "anthem", "instrumental"]
    themes JSONB NOT NULL DEFAULT '[]'::jsonb,  -- e.g., ["love", "adventure", "introspection"]

    -- Acoustic properties
    acousticness FLOAT,  -- 0.0 (electronic) to 1.0 (acoustic)
    instrumentalness FLOAT,  -- 0.0 (vocal) to 1.0 (instrumental)

    -- Popularity and play metrics
    play_count INTEGER NOT NULL DEFAULT 0,
    skip_count INTEGER NOT NULL DEFAULT 0,
    last_played TIMESTAMPTZ,

    -- Ratings and popularity
    user_rating FLOAT,  -- User's personal rating (0.0 to 5.0)
    avg_rating FLOAT,  -- Average rating from all users
    rating_count INTEGER NOT NULL DEFAULT 0,

    -- External metadata references
    musicbrainz_id VARCHAR(100),
    rym_rating FLOAT,  -- RateYourMusic rating
    rym_rating_count INTEGER,
    lastfm_playcount INTEGER,
    lastfm_listeners INTEGER,

    -- AI embedding for semantic search (optional, for future ML features)
    embedding VECTOR(1536),  -- Will be NULL until we implement embeddings

    -- Metadata
    ai_analyzed BOOLEAN NOT NULL DEFAULT false,
    ai_analysis_version INTEGER DEFAULT 1,
    last_synced TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_ai_analysis TIMESTAMPTZ,

    -- Full text search
    search_vector tsvector GENERATED ALWAYS AS (
        setweight(to_tsvector('english', coalesce(title, '')), 'A') ||
        setweight(to_tsvector('english', coalesce(artist, '')), 'B') ||
        setweight(to_tsvector('english', coalesce(album, '')), 'C')
    ) STORED
);

-- Indexes for fast querying
CREATE INDEX idx_library_index_artist ON library_index USING gin(artist gin_trgm_ops);
CREATE INDEX idx_library_index_title ON library_index USING gin(title gin_trgm_ops);
CREATE INDEX idx_library_index_album ON library_index USING gin(album gin_trgm_ops);
CREATE INDEX idx_library_index_genres ON library_index USING gin(genres);
CREATE INDEX idx_library_index_mood_tags ON library_index USING gin(mood_tags);
CREATE INDEX idx_library_index_song_type ON library_index USING gin(song_type);
CREATE INDEX idx_library_index_themes ON library_index USING gin(themes);
CREATE INDEX idx_library_index_year ON library_index(year) WHERE year IS NOT NULL;
CREATE INDEX idx_library_index_energy ON library_index(energy_level) WHERE energy_level IS NOT NULL;
CREATE INDEX idx_library_index_valence ON library_index(valence) WHERE valence IS NOT NULL;
CREATE INDEX idx_library_index_search ON library_index USING gin(search_vector);

-- Library statistics table (aggregated data for AI context)
CREATE TABLE library_stats (
    id SERIAL PRIMARY KEY,

    -- Overall counts
    total_tracks INTEGER NOT NULL DEFAULT 0,
    total_artists INTEGER NOT NULL DEFAULT 0,
    total_albums INTEGER NOT NULL DEFAULT 0,

    -- Genre distribution (top genres with counts)
    genre_distribution JSONB NOT NULL DEFAULT '{}'::jsonb,

    -- Artist distribution (top artists with track counts)
    artist_distribution JSONB NOT NULL DEFAULT '{}'::jsonb,

    -- Year range
    earliest_year INTEGER,
    latest_year INTEGER,
    year_distribution JSONB NOT NULL DEFAULT '{}'::jsonb,

    -- Mood analysis
    mood_distribution JSONB NOT NULL DEFAULT '{}'::jsonb,

    -- Energy/tempo averages
    avg_energy FLOAT,
    avg_tempo FLOAT,
    avg_valence FLOAT,

    -- Song type distribution
    song_type_distribution JSONB NOT NULL DEFAULT '{}'::jsonb,

    -- Metadata
    computed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    total_ai_analyzed INTEGER NOT NULL DEFAULT 0,
    ai_analysis_percentage FLOAT NOT NULL DEFAULT 0.0
);

-- Sync status tracking
CREATE TABLE library_sync_status (
    id SERIAL PRIMARY KEY,

    -- Sync tracking
    last_full_sync TIMESTAMPTZ,
    last_incremental_sync TIMESTAMPTZ,
    sync_in_progress BOOLEAN NOT NULL DEFAULT false,

    -- Progress tracking
    total_tracks_in_navidrome INTEGER NOT NULL DEFAULT 0,
    tracks_synced INTEGER NOT NULL DEFAULT 0,
    tracks_analyzed INTEGER NOT NULL DEFAULT 0,

    -- Error tracking
    last_sync_error TEXT,
    last_sync_error_at TIMESTAMPTZ,

    -- Version tracking
    navidrome_version VARCHAR(50),
    current_ai_version INTEGER NOT NULL DEFAULT 1,

    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Insert initial sync status
INSERT INTO library_sync_status (id) VALUES (1);

-- Function to update library statistics
CREATE OR REPLACE FUNCTION update_library_stats()
RETURNS void AS $$
BEGIN
    INSERT INTO library_stats (
        total_tracks,
        total_artists,
        total_albums,
        genre_distribution,
        artist_distribution,
        earliest_year,
        latest_year,
        year_distribution,
        mood_distribution,
        avg_energy,
        avg_tempo,
        avg_valence,
        song_type_distribution,
        total_ai_analyzed,
        ai_analysis_percentage
    )
    SELECT
        COUNT(*) as total_tracks,
        COUNT(DISTINCT artist) as total_artists,
        COUNT(DISTINCT album) as total_albums,

        -- Genre distribution
        (SELECT jsonb_object_agg(genre, cnt)
         FROM (
             SELECT genre, COUNT(*) as cnt
             FROM library_index, jsonb_array_elements_text(genres) as genre
             GROUP BY genre
             ORDER BY cnt DESC
             LIMIT 50
         ) g) as genre_distribution,

        -- Artist distribution
        (SELECT jsonb_object_agg(artist, cnt)
         FROM (
             SELECT artist, COUNT(*) as cnt
             FROM library_index
             GROUP BY artist
             ORDER BY cnt DESC
             LIMIT 100
         ) a) as artist_distribution,

        MIN(year) as earliest_year,
        MAX(year) as latest_year,

        -- Year distribution
        (SELECT jsonb_object_agg(year::text, cnt)
         FROM (
             SELECT year, COUNT(*) as cnt
             FROM library_index
             WHERE year IS NOT NULL
             GROUP BY year
             ORDER BY year
         ) y) as year_distribution,

        -- Mood distribution
        (SELECT jsonb_object_agg(mood, cnt)
         FROM (
             SELECT mood, COUNT(*) as cnt
             FROM library_index, jsonb_array_elements_text(mood_tags) as mood
             GROUP BY mood
             ORDER BY cnt DESC
             LIMIT 50
         ) m) as mood_distribution,

        AVG(energy_level) as avg_energy,
        AVG(tempo) as avg_tempo,
        AVG(valence) as avg_valence,

        -- Song type distribution
        (SELECT jsonb_object_agg(song_type, cnt)
         FROM (
             SELECT song_type, COUNT(*) as cnt
             FROM library_index, jsonb_array_elements_text(song_type) as song_type
             GROUP BY song_type
             ORDER BY cnt DESC
             LIMIT 50
         ) st) as song_type_distribution,

        COUNT(*) FILTER (WHERE ai_analyzed = true) as total_ai_analyzed,
        (COUNT(*) FILTER (WHERE ai_analyzed = true)::float / GREATEST(COUNT(*), 1) * 100) as ai_analysis_percentage
    FROM library_index
    ON CONFLICT (id) DO UPDATE SET
        total_tracks = EXCLUDED.total_tracks,
        total_artists = EXCLUDED.total_artists,
        total_albums = EXCLUDED.total_albums,
        genre_distribution = EXCLUDED.genre_distribution,
        artist_distribution = EXCLUDED.artist_distribution,
        earliest_year = EXCLUDED.earliest_year,
        latest_year = EXCLUDED.latest_year,
        year_distribution = EXCLUDED.year_distribution,
        mood_distribution = EXCLUDED.mood_distribution,
        avg_energy = EXCLUDED.avg_energy,
        avg_tempo = EXCLUDED.avg_tempo,
        avg_valence = EXCLUDED.avg_valence,
        song_type_distribution = EXCLUDED.song_type_distribution,
        total_ai_analyzed = EXCLUDED.total_ai_analyzed,
        ai_analysis_percentage = EXCLUDED.ai_analysis_percentage,
        computed_at = NOW();
END;
$$ LANGUAGE plpgsql;

-- External metadata sources (RYM, Last.fm, MusicBrainz, etc.)
CREATE TABLE external_metadata (
    id SERIAL PRIMARY KEY,
    track_id VARCHAR(100) NOT NULL REFERENCES library_index(id) ON DELETE CASCADE,
    source VARCHAR(50) NOT NULL,  -- 'rym', 'lastfm', 'musicbrainz', 'spotify', etc.

    -- Ratings
    rating FLOAT,
    rating_count INTEGER,
    popularity_score FLOAT,

    -- Additional metadata
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,

    -- Tags/descriptors from external source
    tags JSONB NOT NULL DEFAULT '[]'::jsonb,

    -- Fetch tracking
    fetched_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    fetch_error TEXT,

    UNIQUE(track_id, source)
);

CREATE INDEX idx_external_metadata_track ON external_metadata(track_id);
CREATE INDEX idx_external_metadata_source ON external_metadata(source);

-- User ratings table (for personalized recommendations)
CREATE TABLE user_track_ratings (
    id SERIAL PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    track_id VARCHAR(100) NOT NULL REFERENCES library_index(id) ON DELETE CASCADE,

    rating FLOAT NOT NULL CHECK (rating >= 0 AND rating <= 5),
    rated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    UNIQUE(user_id, track_id)
);

CREATE INDEX idx_user_track_ratings_user ON user_track_ratings(user_id);
CREATE INDEX idx_user_track_ratings_track ON user_track_ratings(track_id);
CREATE INDEX idx_user_track_ratings_rating ON user_track_ratings(rating);

-- Trigger to update library_index ratings when user ratings change
CREATE OR REPLACE FUNCTION update_track_ratings()
RETURNS TRIGGER AS $$
BEGIN
    UPDATE library_index
    SET
        avg_rating = (SELECT AVG(rating) FROM user_track_ratings WHERE track_id = NEW.track_id),
        rating_count = (SELECT COUNT(*) FROM user_track_ratings WHERE track_id = NEW.track_id)
    WHERE id = NEW.track_id;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trigger_update_track_ratings
AFTER INSERT OR UPDATE OR DELETE ON user_track_ratings
FOR EACH ROW EXECUTE FUNCTION update_track_ratings();

-- AI query cache (to avoid re-analyzing same queries)
CREATE TABLE ai_query_cache (
    id SERIAL PRIMARY KEY,
    query_hash VARCHAR(64) UNIQUE NOT NULL,  -- MD5/SHA256 of normalized query
    original_query TEXT NOT NULL,

    -- AI analysis results
    analyzed_filters JSONB NOT NULL,  -- Structured filters extracted from query
    semantic_intent TEXT,  -- What the user is trying to find

    -- Metadata
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_used TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    use_count INTEGER NOT NULL DEFAULT 1,
    ai_model_version VARCHAR(50)
);

CREATE INDEX idx_ai_query_cache_hash ON ai_query_cache(query_hash);
CREATE INDEX idx_ai_query_cache_last_used ON ai_query_cache(last_used);

-- Function to clean old cache entries (keep last 1000 or last 30 days)
CREATE OR REPLACE FUNCTION clean_ai_query_cache()
RETURNS INTEGER AS $$
DECLARE
    deleted_count INTEGER;
BEGIN
    WITH to_delete AS (
        SELECT id FROM ai_query_cache
        WHERE created_at < NOW() - INTERVAL '30 days'
        AND id NOT IN (
            SELECT id FROM ai_query_cache
            ORDER BY last_used DESC
            LIMIT 1000
        )
    )
    DELETE FROM ai_query_cache
    WHERE id IN (SELECT id FROM to_delete);

    GET DIAGNOSTICS deleted_count = ROW_COUNT;
    RETURN deleted_count;
END;
$$ LANGUAGE plpgsql;
