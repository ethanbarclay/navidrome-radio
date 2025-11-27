use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct LibraryTrack {
    pub id: String,

    // Basic metadata
    pub title: String,
    pub artist: String,
    pub album: String,
    pub album_artist: Option<String>,
    pub composer: Option<String>,
    pub year: Option<i32>,
    pub duration: i32,

    // Genre and categorization
    #[sqlx(json)]
    pub genres: Vec<String>,

    // AI-analyzed metadata
    #[sqlx(json)]
    pub mood_tags: Vec<String>,
    pub energy_level: Option<f64>,
    pub danceability: Option<f64>,
    pub valence: Option<f64>,
    pub tempo: Option<f64>,

    // Categorization
    #[sqlx(json)]
    pub song_type: Vec<String>,
    #[sqlx(json)]
    pub themes: Vec<String>,

    // Acoustic properties
    pub acousticness: Option<f64>,
    pub instrumentalness: Option<f64>,

    // Popularity and play metrics
    pub play_count: i32,
    pub skip_count: i32,
    pub last_played: Option<DateTime<Utc>>,

    // Ratings
    pub user_rating: Option<f64>,
    pub avg_rating: Option<f64>,
    pub rating_count: i32,

    // External metadata
    pub musicbrainz_id: Option<String>,
    pub rym_rating: Option<f64>,
    pub rym_rating_count: Option<i32>,
    pub lastfm_playcount: Option<i32>,
    pub lastfm_listeners: Option<i32>,

    // Metadata
    pub ai_analyzed: bool,
    pub ai_analysis_version: Option<i32>,
    pub last_synced: DateTime<Utc>,
    pub last_ai_analysis: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryStats {
    pub total_tracks: i32,
    pub total_artists: i32,
    pub total_albums: i32,
    pub genre_distribution: serde_json::Value,
    pub artist_distribution: serde_json::Value,
    pub earliest_year: Option<i32>,
    pub latest_year: Option<i32>,
    pub year_distribution: serde_json::Value,
    pub mood_distribution: serde_json::Value,
    pub avg_energy: Option<f64>,
    pub avg_tempo: Option<f64>,
    pub avg_valence: Option<f64>,
    pub song_type_distribution: serde_json::Value,
    pub total_ai_analyzed: i32,
    pub ai_analysis_percentage: f64,
    pub computed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct LibrarySyncStatus {
    pub id: i32,
    pub last_full_sync: Option<DateTime<Utc>>,
    pub last_incremental_sync: Option<DateTime<Utc>>,
    pub sync_in_progress: bool,
    pub total_tracks_in_navidrome: i32,
    pub tracks_synced: i32,
    pub tracks_analyzed: i32,
    pub last_sync_error: Option<String>,
    pub last_sync_error_at: Option<DateTime<Utc>>,
    pub navidrome_version: Option<String>,
    pub current_ai_version: i32,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ExternalMetadata {
    pub id: i32,
    pub track_id: String,
    pub source: String,
    pub rating: Option<f32>,
    pub rating_count: Option<i32>,
    pub popularity_score: Option<f32>,
    #[sqlx(json)]
    pub metadata: serde_json::Value,
    #[sqlx(json)]
    pub tags: Vec<String>,
    pub fetched_at: DateTime<Utc>,
    pub fetch_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct UserTrackRating {
    pub id: i32,
    pub user_id: Uuid,
    pub track_id: String,
    pub rating: f32,
    pub rated_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct AiQueryCache {
    pub id: i32,
    pub query_hash: String,
    pub original_query: String,
    #[sqlx(json)]
    pub analyzed_filters: serde_json::Value,
    pub semantic_intent: Option<String>,
    pub created_at: DateTime<Utc>,
    pub last_used: DateTime<Utc>,
    pub use_count: i32,
    pub ai_model_version: Option<String>,
}

// Request/Response types for AI analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackAnalysisRequest {
    pub track_id: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub genres: Vec<String>,
    pub year: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackAnalysisResult {
    pub mood_tags: Vec<String>,
    pub energy_level: Option<f64>,
    pub danceability: Option<f64>,
    pub valence: Option<f64>,
    pub song_type: Vec<String>,
    pub themes: Vec<String>,
    pub acousticness: Option<f64>,
    pub instrumentalness: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryAnalysisRequest {
    pub query: String,
    pub library_context: LibraryStats,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryAnalysisResult {
    pub semantic_intent: String,
    pub filters: QueryFilters,
    pub confidence: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryFilters {
    pub genres: Option<Vec<String>>,
    pub artists: Option<Vec<String>>,
    pub moods: Option<Vec<String>>,
    pub themes: Option<Vec<String>>,
    pub song_types: Option<Vec<String>>,
    pub year_range: Option<(i32, i32)>,
    pub energy_range: Option<(f32, f32)>,
    pub tempo_range: Option<(f32, f32)>,
    pub valence_range: Option<(f32, f32)>,
    pub min_rating: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackSelectionRequest {
    pub filters: QueryFilters,
    pub library_tracks: Vec<LibraryTrack>,
    pub limit: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackSelectionResult {
    pub selected_tracks: Vec<String>,  // Track IDs
    pub scores: Vec<f32>,  // Relevance scores for each track
    pub reasoning: String,  // AI explanation of selection
}

/// Progress update for library sync operations
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SyncProgress {
    #[serde(rename = "started")]
    Started {
        message: String,
    },
    #[serde(rename = "fetching")]
    Fetching {
        iteration: usize,
        message: String,
    },
    #[serde(rename = "processing")]
    Processing {
        current: usize,
        total: usize,
        new_tracks: usize,
        message: String,
    },
    #[serde(rename = "stats")]
    ComputingStats {
        message: String,
    },
    #[serde(rename = "completed")]
    Completed {
        total_tracks: usize,
        message: String,
    },
    #[serde(rename = "error")]
    Error {
        message: String,
    },
}

/// Progress update for AI curation operations
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "step")]
pub enum CurationProgress {
    #[serde(rename = "started")]
    Started {
        query: String,
        message: String,
    },
    #[serde(rename = "checking_cache")]
    CheckingCache {
        message: String,
    },
    #[serde(rename = "analyzing_library")]
    AnalyzingLibrary {
        message: String,
    },
    #[serde(rename = "ai_analyzing_query")]
    AiAnalyzingQuery {
        message: String,
        thinking: Option<String>,
    },
    #[serde(rename = "searching_tracks")]
    SearchingTracks {
        message: String,
        filters_applied: Option<serde_json::Value>,
    },
    #[serde(rename = "ai_selecting_tracks")]
    AiSelectingTracks {
        message: String,
        candidate_count: usize,
        thinking: Option<String>,
    },
    #[serde(rename = "validating")]
    Validating {
        message: String,
        tracks_validated: usize,
        tracks_rejected: usize,
    },
    #[serde(rename = "completed")]
    Completed {
        message: String,
        tracks_selected: usize,
        reasoning: Option<String>,
    },
    #[serde(rename = "error")]
    Error {
        message: String,
    },
}
