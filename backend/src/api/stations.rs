use crate::api::middleware::{RequireAdmin, RequireAuth};
use crate::error::{AppError, Result};
use crate::models::{CreateStationRequest, CurationProgress, NowPlaying, Station, UpdateStationRequest};
use crate::services::{
    audio_encoder::AudioEncoder,
    hybrid_curator::HybridCurator,
    library_indexer::LibraryIndexer,
    AiCurator, AuthService, CurationEngine, NavidromeClient, StationManager,
};
use axum::{
    extract::{Path, State},
    response::sse::{Event, KeepAlive, Sse},
    routing::{get, post},
    Json, Router,
};
use futures::{stream::Stream, StreamExt};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::{convert::Infallible, sync::Arc};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use uuid::Uuid;
use validator::Validate;

/// State for controlling embedding indexing
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmbeddingControlState {
    Idle,
    Running,
    Paused,
    Stopping,
}

impl Default for EmbeddingControlState {
    fn default() -> Self {
        Self::Idle
    }
}

pub struct AppState {
    pub db: PgPool,
    pub auth_service: Arc<AuthService>,
    pub station_manager: Arc<StationManager>,
    pub curation_engine: Arc<CurationEngine>,
    pub library_indexer: Arc<LibraryIndexer>,
    pub ai_curator: Option<Arc<AiCurator>>,
    pub audio_encoder: Option<Arc<AudioEncoder>>,
    pub hybrid_curator: Option<Arc<HybridCurator>>,
    pub navidrome_client: Arc<NavidromeClient>,
    pub navidrome_library_path: Option<String>,
    pub embedding_control: Arc<tokio::sync::RwLock<EmbeddingControlState>>,
}

#[derive(Debug, Serialize)]
struct AiCapabilities {
    available: bool,
    features: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct AnalyzeDescriptionRequest {
    description: String,
}

#[derive(Debug, Serialize)]
struct AnalyzeDescriptionResponse {
    genres: Vec<String>,
    tracks_found: usize,
    sample_tracks: Vec<String>,
}

pub fn station_routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/stations", get(list_stations).post(create_station))
        .route("/stations/listeners", get(get_all_listener_counts))  // Must be before :id route
        .route("/stations/:id", get(get_station).patch(update_station).delete(delete_station))
        .route("/stations/:id/start", post(start_station))
        .route("/stations/:id/stop", post(stop_station))
        .route("/stations/:id/skip", post(skip_track))
        .route("/stations/:id/nowplaying", get(now_playing))
        .route("/stations/:id/tracks", get(get_station_tracks))
        .route("/stations/:id/playlist", post(create_navidrome_playlist))
        .route("/stations/:id/listener/heartbeat", post(listener_heartbeat))
        .route("/stations/:id/listener/leave", post(listener_leave))
        .route("/ai/capabilities", get(ai_capabilities))
        .route("/ai/analyze-description", post(analyze_description))
        .route("/ai/curate", post(curate_tracks_sse))
}

async fn list_stations(State(state): State<Arc<AppState>>) -> Result<Json<Vec<Station>>> {
    let stations = sqlx::query_as::<_, Station>("SELECT * FROM stations ORDER BY created_at DESC")
        .fetch_all(&state.db)
        .await?;

    Ok(Json(stations))
}

async fn get_station(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<Json<Station>> {
    let station = sqlx::query_as::<_, Station>("SELECT * FROM stations WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound("Station not found".to_string()))?;

    Ok(Json(station))
}

async fn create_station(
    State(state): State<Arc<AppState>>,
    RequireAdmin(claims): RequireAdmin,
    Json(req): Json<CreateStationRequest>,
) -> Result<Json<Station>> {
    req.validate()
        .map_err(|e| AppError::Validation(e.to_string()))?;

    // Debug: Log incoming track_ids
    let track_count = req.track_ids.as_ref().map(|t| t.len()).unwrap_or(0);
    tracing::info!("Creating station '{}' with {} track_ids", req.name, track_count);

    // Check if path is unique
    let exists = sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM stations WHERE path = $1)")
        .bind(&req.path)
        .fetch_one(&state.db)
        .await?;

    if exists {
        return Err(AppError::Validation("Station path already exists".to_string()));
    }

    let config = req.config.unwrap_or_default();
    let track_ids = req.track_ids.unwrap_or_default();

    let station = sqlx::query_as::<_, Station>(
        r#"
        INSERT INTO stations (path, name, description, genres, mood_tags, created_by, config, track_ids)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        RETURNING *
        "#,
    )
    .bind(&req.path)
    .bind(&req.name)
    .bind(&req.description)
    .bind(serde_json::to_value(&req.genres).unwrap())
    .bind(serde_json::to_value(&req.mood_tags.unwrap_or_default()).unwrap())
    .bind(claims.sub)
    .bind(serde_json::to_value(&config).unwrap())
    .bind(serde_json::to_value(&track_ids).unwrap())
    .fetch_one(&state.db)
    .await?;

    Ok(Json(station))
}

async fn update_station(
    State(state): State<Arc<AppState>>,
    RequireAdmin(_): RequireAdmin,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateStationRequest>,
) -> Result<Json<Station>> {
    // Build dynamic update query
    let mut query = String::from("UPDATE stations SET ");
    let mut updates = Vec::new();
    let mut param_count = 1;

    if req.name.is_some() {
        updates.push(format!("name = ${}", param_count));
        param_count += 1;
    }
    if req.description.is_some() {
        updates.push(format!("description = ${}", param_count));
        param_count += 1;
    }
    if req.genres.is_some() {
        updates.push(format!("genres = ${}", param_count));
        param_count += 1;
    }
    if req.mood_tags.is_some() {
        updates.push(format!("mood_tags = ${}", param_count));
        param_count += 1;
    }
    if req.config.is_some() {
        updates.push(format!("config = ${}", param_count));
        param_count += 1;
    }

    if updates.is_empty() {
        return Err(AppError::Validation("No fields to update".to_string()));
    }

    query.push_str(&updates.join(", "));
    query.push_str(&format!(" WHERE id = ${} RETURNING *", param_count));

    let mut query_builder = sqlx::query_as::<_, Station>(&query);

    if let Some(name) = req.name {
        query_builder = query_builder.bind(name);
    }
    if let Some(description) = req.description {
        query_builder = query_builder.bind(description);
    }
    if let Some(genres) = req.genres {
        query_builder = query_builder.bind(serde_json::to_value(genres).unwrap());
    }
    if let Some(mood_tags) = req.mood_tags {
        query_builder = query_builder.bind(serde_json::to_value(mood_tags).unwrap());
    }
    if let Some(config) = req.config {
        query_builder = query_builder.bind(serde_json::to_value(config).unwrap());
    }

    let station = query_builder
        .bind(id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound("Station not found".to_string()))?;

    Ok(Json(station))
}

async fn delete_station(
    State(state): State<Arc<AppState>>,
    RequireAdmin(_): RequireAdmin,
    Path(id): Path<Uuid>,
) -> Result<Json<()>> {
    // Stop station if active
    let _ = state.station_manager.stop_station(id).await;

    sqlx::query("DELETE FROM stations WHERE id = $1")
        .bind(id)
        .execute(&state.db)
        .await?;

    Ok(Json(()))
}

async fn start_station(
    State(state): State<Arc<AppState>>,
    RequireAdmin(_): RequireAdmin,
    Path(id): Path<Uuid>,
) -> Result<Json<()>> {
    state.station_manager.start_station(id).await?;
    Ok(Json(()))
}

async fn stop_station(
    State(state): State<Arc<AppState>>,
    RequireAdmin(_): RequireAdmin,
    Path(id): Path<Uuid>,
) -> Result<Json<()>> {
    state.station_manager.stop_station(id).await?;
    Ok(Json(()))
}

async fn skip_track(
    State(state): State<Arc<AppState>>,
    RequireAdmin(_): RequireAdmin,
    Path(id): Path<Uuid>,
) -> Result<Json<()>> {
    state.station_manager.skip_track(id).await?;
    Ok(Json(()))
}

async fn now_playing(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<Json<NowPlaying>> {
    let np = state.station_manager.get_now_playing(id).await?;
    Ok(Json(np))
}

#[derive(Debug, Deserialize)]
struct HeartbeatRequest {
    session_id: String,
}

#[derive(Debug, Serialize)]
struct HeartbeatResponse {
    listeners: usize,
}

async fn listener_heartbeat(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    Json(req): Json<HeartbeatRequest>,
) -> Result<Json<HeartbeatResponse>> {
    let listeners = state
        .station_manager
        .listener_heartbeat(id, req.session_id)
        .await?;
    Ok(Json(HeartbeatResponse { listeners }))
}

#[derive(Debug, Deserialize)]
struct LeaveRequest {
    session_id: String,
}

async fn listener_leave(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    Json(req): Json<LeaveRequest>,
) -> Result<Json<()>> {
    state
        .station_manager
        .listener_leave(id, &req.session_id)
        .await?;
    Ok(Json(()))
}

#[derive(Debug, Serialize)]
struct ListenerCountsResponse {
    counts: std::collections::HashMap<Uuid, usize>,
}

async fn get_all_listener_counts(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ListenerCountsResponse>> {
    let counts = state.station_manager.get_all_listener_counts().await;
    Ok(Json(ListenerCountsResponse { counts }))
}

#[derive(Debug, Deserialize)]
struct GetTracksQuery {
    limit: Option<i64>,
}

#[derive(Debug, Serialize)]
struct StationTrack {
    id: String,
    title: String,
    artist: String,
    album: String,
    played_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Serialize)]
struct StationTracksResponse {
    tracks: Vec<StationTrack>,
    total: i64,
}

async fn get_station_tracks(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    axum::extract::Query(query): axum::extract::Query<GetTracksQuery>,
) -> Result<Json<StationTracksResponse>> {
    let limit = query.limit.unwrap_or(50).min(200);

    // First get the station to access its curated track_ids
    let station = sqlx::query_as::<_, Station>("SELECT * FROM stations WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound("Station not found".to_string()))?;

    // If station has curated track_ids, show those in the original curated order
    if !station.track_ids.is_empty() {
        let track_ids = &station.track_ids;
        let total = track_ids.len() as i64;

        // Get track details from library_index, preserving the curated order
        // We use a CTE with ordinality to maintain the exact playlist order
        let rows = sqlx::query(
            r#"
            WITH ordered_ids AS (
                SELECT id, ord
                FROM UNNEST($1::text[]) WITH ORDINALITY AS t(id, ord)
            )
            SELECT li.id, li.title, li.artist, li.album, oi.ord
            FROM ordered_ids oi
            JOIN library_index li ON li.id = oi.id
            ORDER BY oi.ord
            LIMIT $2
            "#,
        )
        .bind(track_ids)
        .bind(limit)
        .fetch_all(&state.db)
        .await?;

        let tracks: Vec<StationTrack> = rows
            .iter()
            .map(|row| {
                use sqlx::Row;
                StationTrack {
                    id: row.get("id"),
                    title: row.get("title"),
                    artist: row.get("artist"),
                    album: row.get("album"),
                    played_at: None, // Curated tracks - order is the playlist order
                }
            })
            .collect();

        return Ok(Json(StationTracksResponse { tracks, total }));
    }

    // Fallback: Get tracks from playlist_history for stations without curated tracks
    let rows = sqlx::query(
        r#"
        SELECT
            li.id,
            li.title,
            li.artist,
            li.album,
            ph.played_at
        FROM playlist_history ph
        JOIN library_index li ON ph.track_id = li.id
        WHERE ph.station_id = $1
        ORDER BY ph.played_at DESC
        LIMIT $2
        "#,
    )
    .bind(id)
    .bind(limit)
    .fetch_all(&state.db)
    .await?;

    let tracks: Vec<StationTrack> = rows
        .iter()
        .map(|row| {
            use sqlx::Row;
            StationTrack {
                id: row.get("id"),
                title: row.get("title"),
                artist: row.get("artist"),
                album: row.get("album"),
                played_at: row.get("played_at"),
            }
        })
        .collect();

    // Get total count
    let total: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM playlist_history WHERE station_id = $1"
    )
    .bind(id)
    .fetch_one(&state.db)
    .await?;

    Ok(Json(StationTracksResponse { tracks, total }))
}

#[derive(Debug, Deserialize)]
struct CreatePlaylistRequest {
    name: Option<String>,
}

#[derive(Debug, Serialize)]
struct CreatePlaylistResponse {
    playlist_id: String,
    name: String,
    track_count: usize,
}

/// Create a Navidrome playlist from a station's tracks
async fn create_navidrome_playlist(
    State(state): State<Arc<AppState>>,
    RequireAdmin(_): RequireAdmin,
    Path(id): Path<Uuid>,
    Json(req): Json<CreatePlaylistRequest>,
) -> Result<Json<CreatePlaylistResponse>> {
    // Get the station
    let station = sqlx::query_as::<_, Station>("SELECT * FROM stations WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound("Station not found".to_string()))?;

    // Get track IDs - prefer curated tracks, fall back to playlist history
    let track_ids: Vec<String> = if !station.track_ids.is_empty() {
        station.track_ids.clone()
    } else {
        // Get from playlist history
        let rows: Vec<(String,)> = sqlx::query_as(
            "SELECT track_id FROM playlist_history WHERE station_id = $1 ORDER BY played_at DESC LIMIT 200"
        )
        .bind(id)
        .fetch_all(&state.db)
        .await?;

        rows.into_iter().map(|(id,)| id).collect()
    };

    if track_ids.is_empty() {
        return Err(AppError::Validation("Station has no tracks to export".to_string()));
    }

    // Generate playlist name if not provided
    let playlist_name = req.name.unwrap_or_else(|| {
        format!("{} - Radio", station.name)
    });

    // Create the playlist in Navidrome
    let playlist_id = state
        .navidrome_client
        .create_playlist(&playlist_name, &track_ids)
        .await?;

    Ok(Json(CreatePlaylistResponse {
        playlist_id,
        name: playlist_name,
        track_count: track_ids.len(),
    }))
}

async fn ai_capabilities(State(state): State<Arc<AppState>>) -> Result<Json<AiCapabilities>> {
    let available = state.curation_engine.has_ai_capabilities();

    let features = if available {
        vec![
            "AI-powered station creation from descriptions".to_string(),
            "Natural language genre extraction".to_string(),
            "Contextual track selection (coming soon)".to_string(),
        ]
    } else {
        vec![]
    };

    Ok(Json(AiCapabilities { available, features }))
}

async fn analyze_description(
    State(state): State<Arc<AppState>>,
    RequireAdmin(_): RequireAdmin,
    Json(req): Json<AnalyzeDescriptionRequest>,
) -> Result<Json<AnalyzeDescriptionResponse>> {
    if req.description.trim().is_empty() {
        return Err(AppError::Validation("Description cannot be empty".to_string()));
    }

    let (genres, tracks) = state
        .curation_engine
        .analyze_description_and_find_tracks(&req.description)
        .await?;

    // Get sample track titles (first 5)
    let sample_tracks: Vec<String> = tracks
        .iter()
        .take(5)
        .map(|t| format!("{} - {}", t.artist, t.title))
        .collect();

    Ok(Json(AnalyzeDescriptionResponse {
        genres,
        tracks_found: tracks.len(),
        sample_tracks,
    }))
}

#[derive(Debug, Deserialize)]
struct CurateRequest {
    query: String,
    #[serde(default = "default_limit")]
    limit: usize,
}

fn default_limit() -> usize {
    50
}

#[derive(Debug, Serialize)]
struct CurationResult {
    track_ids: Vec<String>,
}

/// SSE endpoint for AI curation with real-time progress updates
async fn curate_tracks_sse(
    State(state): State<Arc<AppState>>,
    RequireAdmin(_): RequireAdmin,
    Json(req): Json<CurateRequest>,
) -> Result<Sse<impl Stream<Item = std::result::Result<Event, Infallible>>>> {
    let ai_curator = state.ai_curator.clone().ok_or_else(|| {
        AppError::ExternalApi("AI curation not available (no API key configured)".to_string())
    })?;

    let query = req.query.clone();
    let limit = req.limit;

    // Create a channel for progress updates
    let (progress_tx, progress_rx) = mpsc::channel::<CurationProgress>(32);

    // Spawn the curation task
    tokio::spawn(async move {
        let result = ai_curator
            .curate_tracks_with_progress(query, limit, progress_tx.clone())
            .await;

        // Send final result or error
        match result {
            Ok(track_ids) => {
                // The completed message is already sent by the curator
                // But we can send the actual result as a separate event
                let _ = progress_tx
                    .send(CurationProgress::Completed {
                        message: format!("Selected {} tracks", track_ids.len()),
                        tracks_selected: track_ids.len(),
                        reasoning: Some(serde_json::to_string(&CurationResult { track_ids }).unwrap_or_default()),
                    })
                    .await;
            }
            Err(e) => {
                let _ = progress_tx
                    .send(CurationProgress::Error {
                        message: e.to_string(),
                    })
                    .await;
            }
        }
    });

    // Convert the receiver to an SSE stream
    let stream = ReceiverStream::new(progress_rx).map(|progress| {
        let data = serde_json::to_string(&progress).unwrap_or_else(|_| "{}".to_string());
        Ok(Event::default().data(data))
    });

    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}
