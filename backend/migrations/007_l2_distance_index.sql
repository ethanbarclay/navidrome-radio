-- Migration: Switch from cosine distance to L2 distance for better similarity spread
-- Embeddings are now normalized to unit length, so L2 distance provides better separation

-- Drop old cosine index
DROP INDEX IF EXISTS idx_track_embeddings_vector;

-- Create new L2 distance index
-- For normalized vectors, L2 distance ranges [0, 2] where 0=identical, sqrt(2)=orthogonal, 2=opposite
CREATE INDEX idx_track_embeddings_vector
ON track_embeddings
USING ivfflat (embedding vector_l2_ops) WITH (lists = 100);

-- Clear existing embeddings since they need to be regenerated with normalization
-- This ensures all embeddings use the same normalization
DELETE FROM track_embeddings;
DELETE FROM embedding_failures;

-- Update processing status to trigger re-indexing
UPDATE embedding_processing_status
SET
    is_processing = false,
    tracks_with_embeddings = 0,
    tracks_pending = 0,
    tracks_failed = 0,
    updated_at = NOW(),
    last_error = 'Cleared for L2 normalization migration'
WHERE id = 1;
