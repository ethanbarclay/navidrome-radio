-- Enable extensions
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";

-- Users table
CREATE TABLE users (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    username VARCHAR(50) UNIQUE NOT NULL,
    email VARCHAR(255) UNIQUE NOT NULL,
    password_hash VARCHAR(255) NOT NULL,
    role TEXT NOT NULL CHECK (role IN ('admin', 'listener')),
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
    genres JSONB NOT NULL,
    mood_tags JSONB NOT NULL DEFAULT '[]'::jsonb,
    created_by UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    active BOOLEAN NOT NULL DEFAULT false,
    config JSONB NOT NULL
);

CREATE INDEX idx_stations_path ON stations(path);
CREATE INDEX idx_stations_active ON stations(active);

-- Tracks table (cached from Navidrome)
CREATE TABLE tracks (
    id VARCHAR(100) PRIMARY KEY,
    title VARCHAR(500) NOT NULL,
    artist VARCHAR(500) NOT NULL,
    album VARCHAR(500) NOT NULL,
    genre JSONB NOT NULL DEFAULT '[]'::jsonb,
    year INTEGER,
    duration INTEGER NOT NULL,
    path TEXT NOT NULL,
    metadata JSONB,
    last_synced TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_tracks_artist ON tracks(artist);

-- Playlist history (tracks played on each station)
CREATE TABLE playlist_history (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    station_id UUID NOT NULL REFERENCES stations(id) ON DELETE CASCADE,
    track_id VARCHAR(100) NOT NULL,
    played_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    selection_method VARCHAR(50) NOT NULL,
    skipped BOOLEAN NOT NULL DEFAULT false
);

CREATE INDEX idx_playlist_history_station ON playlist_history(station_id, played_at DESC);
CREATE INDEX idx_playlist_history_track ON playlist_history(track_id);

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
