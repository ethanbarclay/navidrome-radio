use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Track {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    #[sqlx(json)]
    pub genre: Vec<String>,
    pub year: Option<i32>,
    pub duration: i32,
    pub path: String,
    #[sqlx(json)]
    pub metadata: Option<HashMap<String, String>>,
    pub last_synced: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NowPlaying {
    pub track: TrackInfo,
    pub started_at: DateTime<Utc>,
    pub listeners: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackInfo {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration: i32,
    #[serde(rename = "albumArt")]
    pub album_art: Option<String>,
}

impl From<Track> for TrackInfo {
    fn from(track: Track) -> Self {
        TrackInfo {
            id: track.id.clone(),
            title: track.title,
            artist: track.artist,
            album: track.album,
            duration: track.duration,
            album_art: Some(format!("/api/v1/navidrome/cover/{}", track.id)),
        }
    }
}
