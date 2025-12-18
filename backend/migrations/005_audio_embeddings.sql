-- Audio Embeddings for ML-based music similarity
-- Uses pgvector for efficient vector similarity search
-- Embeddings generated from teticio/audio-encoder (100-dimensional)

-- Enable pgvector extension for vector similarity search
CREATE EXTENSION IF NOT EXISTS vector;

-- Track audio embeddings table
-- Stores 100-dimensional embeddings from the Deej-AI audio encoder
CREATE TABLE track_embeddings (
    track_id VARCHAR(100) PRIMARY KEY REFERENCES library_index(id) ON DELETE CASCADE,

    -- 100-dimensional embedding from audio encoder
    -- Trained on 1M+ Spotify playlists for musical similarity
    embedding vector(100) NOT NULL,

    -- Metadata
    computed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    model_version VARCHAR(50) NOT NULL DEFAULT 'teticio/audio-encoder-v1',

    -- Processing info
    audio_duration_ms INTEGER,  -- Duration of audio processed
    processing_time_ms INTEGER  -- How long encoding took
);

-- IVFFlat index for fast approximate nearest neighbor search
-- lists = sqrt(n) is a good starting point, we'll use 100 for ~10k tracks
-- Can be recreated with more lists as library grows
CREATE INDEX idx_track_embeddings_vector ON track_embeddings
USING ivfflat (embedding vector_cosine_ops) WITH (lists = 100);

-- Index for finding unprocessed tracks
CREATE INDEX idx_track_embeddings_computed ON track_embeddings(computed_at);

-- Embedding processing status table
CREATE TABLE embedding_processing_status (
    id SERIAL PRIMARY KEY,

    -- Processing state
    is_processing BOOLEAN NOT NULL DEFAULT false,
    last_processing_started TIMESTAMPTZ,
    last_processing_completed TIMESTAMPTZ,

    -- Progress tracking
    total_tracks INTEGER NOT NULL DEFAULT 0,
    tracks_with_embeddings INTEGER NOT NULL DEFAULT 0,
    tracks_pending INTEGER NOT NULL DEFAULT 0,
    tracks_failed INTEGER NOT NULL DEFAULT 0,

    -- Performance stats
    avg_processing_time_ms FLOAT,
    total_processing_time_ms BIGINT DEFAULT 0,

    -- Error tracking
    last_error TEXT,
    last_error_at TIMESTAMPTZ,
    consecutive_errors INTEGER NOT NULL DEFAULT 0,

    -- Model info
    current_model_version VARCHAR(50) DEFAULT 'teticio/audio-encoder-v1',

    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Insert initial status
INSERT INTO embedding_processing_status (id) VALUES (1);

-- Failed embeddings table (for retry logic)
CREATE TABLE embedding_failures (
    id SERIAL PRIMARY KEY,
    track_id VARCHAR(100) NOT NULL REFERENCES library_index(id) ON DELETE CASCADE,

    error_message TEXT NOT NULL,
    error_type VARCHAR(100),  -- 'file_not_found', 'decode_error', 'model_error', etc.

    attempt_count INTEGER NOT NULL DEFAULT 1,
    first_attempt TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_attempt TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Will retry if resolved
    resolved BOOLEAN NOT NULL DEFAULT false,
    resolved_at TIMESTAMPTZ,

    UNIQUE(track_id)
);

CREATE INDEX idx_embedding_failures_unresolved ON embedding_failures(track_id)
WHERE resolved = false;

-- Function to update embedding processing stats
CREATE OR REPLACE FUNCTION update_embedding_stats()
RETURNS void AS $$
BEGIN
    UPDATE embedding_processing_status SET
        total_tracks = (SELECT COUNT(*) FROM library_index),
        tracks_with_embeddings = (SELECT COUNT(*) FROM track_embeddings),
        tracks_pending = (
            SELECT COUNT(*) FROM library_index li
            WHERE NOT EXISTS (SELECT 1 FROM track_embeddings te WHERE te.track_id = li.id)
            AND NOT EXISTS (SELECT 1 FROM embedding_failures ef WHERE ef.track_id = li.id AND ef.resolved = false)
        ),
        tracks_failed = (SELECT COUNT(*) FROM embedding_failures WHERE resolved = false),
        avg_processing_time_ms = (SELECT AVG(processing_time_ms) FROM track_embeddings),
        updated_at = NOW()
    WHERE id = 1;
END;
$$ LANGUAGE plpgsql;

-- Helper function to find similar tracks by embedding
-- Returns track IDs ordered by similarity (cosine distance)
CREATE OR REPLACE FUNCTION find_similar_tracks(
    target_track_id VARCHAR(100),
    exclude_ids VARCHAR(100)[],
    result_limit INTEGER DEFAULT 10
)
RETURNS TABLE(track_id VARCHAR(100), similarity FLOAT) AS $$
BEGIN
    RETURN QUERY
    SELECT
        te.track_id,
        1 - (te.embedding <=> target.embedding) as similarity
    FROM track_embeddings te
    CROSS JOIN (
        SELECT embedding FROM track_embeddings WHERE track_id = target_track_id
    ) target
    WHERE te.track_id != target_track_id
    AND te.track_id != ALL(exclude_ids)
    ORDER BY te.embedding <=> target.embedding
    LIMIT result_limit;
END;
$$ LANGUAGE plpgsql;

-- Helper function to find tracks between two embeddings (for transitions)
-- Uses linear interpolation to find tracks along the path
CREATE OR REPLACE FUNCTION find_transition_tracks(
    from_track_id VARCHAR(100),
    to_track_id VARCHAR(100),
    exclude_ids VARCHAR(100)[],
    result_limit INTEGER DEFAULT 5
)
RETURNS TABLE(track_id VARCHAR(100), position_score FLOAT) AS $$
DECLARE
    from_emb vector(100);
    to_emb vector(100);
    midpoint vector(100);
BEGIN
    -- Get embeddings
    SELECT embedding INTO from_emb FROM track_embeddings WHERE track_id = from_track_id;
    SELECT embedding INTO to_emb FROM track_embeddings WHERE track_id = to_track_id;

    -- Calculate midpoint (simple average for now)
    -- In application code we'll do proper interpolation at multiple points

    RETURN QUERY
    SELECT
        te.track_id,
        -- Score based on how close to the line between from and to
        -- Lower distance to both endpoints = better transition track
        (1 - (te.embedding <=> from_emb)) + (1 - (te.embedding <=> to_emb)) as position_score
    FROM track_embeddings te
    WHERE te.track_id != from_track_id
    AND te.track_id != to_track_id
    AND te.track_id != ALL(exclude_ids)
    ORDER BY
        -- Prefer tracks that are close to both endpoints
        (te.embedding <=> from_emb) + (te.embedding <=> to_emb)
    LIMIT result_limit;
END;
$$ LANGUAGE plpgsql;

-- View for embedding coverage stats
CREATE VIEW embedding_coverage AS
SELECT
    eps.total_tracks,
    eps.tracks_with_embeddings,
    eps.tracks_pending,
    eps.tracks_failed,
    CASE WHEN eps.total_tracks > 0
        THEN ROUND((eps.tracks_with_embeddings::numeric / eps.total_tracks) * 100, 2)
        ELSE 0
    END as coverage_percent,
    eps.avg_processing_time_ms,
    eps.current_model_version,
    eps.updated_at
FROM embedding_processing_status eps
WHERE eps.id = 1;
