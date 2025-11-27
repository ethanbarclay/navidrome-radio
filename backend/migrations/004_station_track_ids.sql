-- Add track_ids column to stations table to store curated playlist
ALTER TABLE stations ADD COLUMN IF NOT EXISTS track_ids JSONB NOT NULL DEFAULT '[]'::jsonb;

-- Add index for efficient lookups
CREATE INDEX IF NOT EXISTS idx_stations_track_ids ON stations USING GIN (track_ids);
