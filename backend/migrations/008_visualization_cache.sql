-- Add 2D visualization cache for embeddings
-- This stores pre-computed PCA projections for fast visualization loading

-- Store the 2D coordinates for each track
ALTER TABLE track_embeddings ADD COLUMN IF NOT EXISTS viz_x REAL;
ALTER TABLE track_embeddings ADD COLUMN IF NOT EXISTS viz_y REAL;

-- Index for quick retrieval of visualization data
CREATE INDEX IF NOT EXISTS idx_track_embeddings_viz ON track_embeddings (viz_x, viz_y) WHERE viz_x IS NOT NULL;

-- Store the PCA transformation matrix and metadata for consistent projections
-- This ensures new embeddings are projected the same way as existing ones
CREATE TABLE IF NOT EXISTS visualization_config (
    id INTEGER PRIMARY KEY DEFAULT 1,
    -- Store the two principal component vectors (each is 100-dim for our embeddings)
    pc1 REAL[] NOT NULL,
    pc2 REAL[] NOT NULL,
    -- Mean vector for centering (100-dim)
    mean_vec REAL[] NOT NULL,
    -- Track count when this was computed
    track_count INTEGER NOT NULL,
    -- When this was last updated
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    -- Ensure only one row
    CONSTRAINT single_row CHECK (id = 1)
);
