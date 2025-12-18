-- Add path column to library_index for audio file access
-- Required for audio embedding generation

ALTER TABLE library_index ADD COLUMN IF NOT EXISTS path VARCHAR(1000);

-- Index for efficient path lookups
CREATE INDEX IF NOT EXISTS idx_library_index_path ON library_index(path) WHERE path IS NOT NULL;
