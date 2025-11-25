use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;
use validator::Validate;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::Type, PartialEq)]
#[sqlx(type_name = "text")]
#[serde(rename_all = "snake_case")]
pub enum SelectionMode {
    #[sqlx(rename = "ai_contextual")]
    AIContextual,
    #[sqlx(rename = "ai_embeddings")]
    AIEmbeddings,
    #[sqlx(rename = "random")]
    Random,
    #[sqlx(rename = "hybrid")]
    Hybrid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StationConfig {
    pub bitrate: u32,
    pub sample_rate: u32,
    pub crossfade_ms: u32,
    pub track_selection_mode: SelectionMode,
    pub min_track_duration: u32,
    pub max_track_duration: u32,
    pub explicit_content: bool,
}

impl Default for StationConfig {
    fn default() -> Self {
        Self {
            bitrate: 192,
            sample_rate: 44100,
            crossfade_ms: 0,
            track_selection_mode: SelectionMode::Random,
            min_track_duration: 60,
            max_track_duration: 600,
            explicit_content: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Station {
    pub id: Uuid,
    pub path: String,
    pub name: String,
    pub description: String,
    #[sqlx(json)]
    pub genres: Vec<String>,
    #[sqlx(json)]
    pub mood_tags: Vec<String>,
    pub created_by: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub active: bool,
    #[sqlx(json)]
    pub config: StationConfig,
}

#[derive(Debug, Deserialize, Validate)]
pub struct CreateStationRequest {
    #[validate(length(min = 1, max = 100))]
    pub path: String,
    #[validate(length(min = 1, max = 255))]
    pub name: String,
    #[validate(length(min = 10, max = 2000))]
    pub description: String,
    #[validate(length(min = 1))]
    pub genres: Vec<String>,
    pub mood_tags: Option<Vec<String>>,
    pub config: Option<StationConfig>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateStationRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub genres: Option<Vec<String>>,
    pub mood_tags: Option<Vec<String>>,
    pub config: Option<StationConfig>,
}
