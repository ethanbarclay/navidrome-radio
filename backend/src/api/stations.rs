use crate::api::middleware::{RequireAdmin, RequireAuth};
use crate::error::{AppError, Result};
use crate::models::{CreateStationRequest, CurationProgress, NowPlaying, Station, UpdateStationRequest};
use crate::services::{
    audio_broadcaster::{AudioBroadcaster, AudioBroadcasterConfig, VisualizationData},
    audio_encoder::AudioEncoder,
    audio_pipeline::{AudioPipeline, AudioPipelineConfig, QueuedTrack},
    hybrid_curator::HybridCurator,
    library_indexer::LibraryIndexer,
    AiCurator, AuthService, CurationEngine, NavidromeClient, StationManager,
};
use axum::{
    body::Body,
    extract::{Path, State},
    http::{header, StatusCode},
    response::{sse::{Event, KeepAlive, Sse}, Response},
    routing::{get, post},
    Json, Router,
};
use futures::{stream::Stream, StreamExt};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::{collections::HashMap, convert::Infallible, sync::Arc};
use tokio::sync::{mpsc, RwLock};
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
    /// Per-station audio broadcasters for HLS streaming
    pub station_broadcasters: Arc<RwLock<HashMap<Uuid, Arc<AudioBroadcaster>>>>,
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
        // HLS Streaming endpoints
        .route("/stations/:id/stream/playlist.m3u8", get(get_hls_playlist))
        .route("/stations/:id/stream/segment/:seq", get(get_hls_segment))
        .route("/stations/:id/stream/visualization", get(visualization_sse))
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
    // Check if there's an active HLS broadcaster - if so, skip in the pipeline
    {
        let broadcasters = state.station_broadcasters.read().await;
        if let Some(broadcaster) = broadcasters.get(&id) {
            if broadcaster.is_running() {
                broadcaster.skip().await?;
                tracing::info!("Skipped track in HLS pipeline for station {}", id);
                return Ok(Json(()));
            }
        }
    }

    // Fall back to station manager skip
    state.station_manager.skip_track(id).await?;
    Ok(Json(()))
}

async fn now_playing(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<Json<NowPlaying>> {
    // Check if there's an active HLS broadcaster - if so, use its current track
    {
        let broadcasters = state.station_broadcasters.read().await;
        if let Some(broadcaster) = broadcasters.get(&id) {
            if broadcaster.is_running() {
                // Try to get current track from broadcaster
                let track_state = broadcaster.current_track().await;

                // If broadcaster is running but no current track yet (cold start),
                // wait briefly for the pipeline to start processing
                let track_state = if track_state.is_none() {
                    // Drop the read lock before sleeping
                    drop(broadcasters);

                    // Wait up to 2 seconds for track to be available
                    let mut attempts = 0;
                    loop {
                        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
                        attempts += 1;

                        let broadcasters = state.station_broadcasters.read().await;
                        if let Some(broadcaster) = broadcasters.get(&id) {
                            if let Some(ts) = broadcaster.current_track().await {
                                break Some(ts);
                            }
                        }

                        if attempts >= 10 {
                            tracing::debug!("Timeout waiting for broadcaster current track");
                            break None;
                        }
                    }
                } else {
                    track_state
                };

                if let Some(track_state) = track_state {
                    // Fetch full track info from the database
                    let row = sqlx::query(
                        r#"
                        SELECT id, title, artist, album, duration
                        FROM library_index
                        WHERE id = $1
                        "#,
                    )
                    .bind(&track_state.track_id)
                    .fetch_optional(&state.db)
                    .await?;

                    if let Some(row) = row {
                        use sqlx::Row;
                        let track_id: String = row.get("id");
                        let info = crate::models::TrackInfo {
                            id: track_id.clone(),
                            title: row.get("title"),
                            artist: row.get("artist"),
                            album: row.get("album"),
                            duration: row.get("duration"),
                            album_art: Some(format!("/api/v1/navidrome/cover/{}", track_id)),
                        };

                        // Get listener count from station manager
                        let listeners = state
                            .station_manager
                            .get_now_playing(id)
                            .await
                            .map(|np| np.listeners)
                            .unwrap_or(0);

                        // Account for HLS buffering latency (~6 seconds for 3 segments at 2s each)
                        // The client is behind the server, so from the client's perspective
                        // less of the track has played
                        const HLS_LATENCY_SECS: i64 = 6;
                        let client_position_secs = (track_state.position_secs as i64 - HLS_LATENCY_SECS).max(0);

                        return Ok(Json(NowPlaying {
                            track: info,
                            started_at: chrono::Utc::now() - chrono::Duration::seconds(client_position_secs),
                            listeners,
                        }));
                    }
                }
            }
        }
    }

    // Fall back to station manager's now playing
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

// ============================================================================
// HLS Streaming Endpoints
// ============================================================================

/// Stored broadcaster with its pipeline for control
pub struct StationBroadcaster {
    pub pipeline: Arc<RwLock<AudioPipeline>>,
    pub broadcaster: Arc<AudioBroadcaster>,
}

/// Get or create the broadcaster for a station
async fn get_or_create_broadcaster(
    state: &Arc<AppState>,
    station_id: Uuid,
) -> Result<Arc<AudioBroadcaster>> {
    // Check if broadcaster already exists and is running
    {
        let broadcasters = state.station_broadcasters.read().await;
        if let Some(broadcaster) = broadcasters.get(&station_id) {
            if broadcaster.is_running() {
                return Ok(broadcaster.clone());
            }
        }
    }

    // Get station and its tracks
    let station = sqlx::query_as::<_, Station>("SELECT * FROM stations WHERE id = $1")
        .bind(station_id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound("Station not found".to_string()))?;

    // Create new pipeline
    let mut pipeline = AudioPipeline::new(
        state.navidrome_client.clone(),
        AudioPipelineConfig::default(),
    );

    // Queue tracks from the station's track list
    if !station.track_ids.is_empty() {
        // Get track info from library_index
        let track_ids = &station.track_ids;
        let rows = sqlx::query(
            r#"
            SELECT id, title, artist
            FROM library_index
            WHERE id = ANY($1)
            "#,
        )
        .bind(track_ids)
        .fetch_all(&state.db)
        .await?;

        // Build a map of track_id -> (title, artist)
        let track_info: std::collections::HashMap<String, (String, String)> = rows
            .iter()
            .map(|row| {
                use sqlx::Row;
                let id: String = row.get("id");
                let title: String = row.get("title");
                let artist: String = row.get("artist");
                (id, (title, artist))
            })
            .collect();

        // Queue tracks in order
        for track_id in track_ids {
            if let Some((title, artist)) = track_info.get(track_id) {
                let queued = QueuedTrack {
                    track_id: track_id.clone(),
                    title: title.clone(),
                    artist: artist.clone(),
                };
                pipeline.queue_track(queued).await?;
            }
        }

        tracing::info!(
            "Queued {} tracks for station {} HLS stream",
            track_ids.len(),
            station.name
        );
    } else {
        // No curated tracks - get from current now playing or playlist history
        let now_playing = state.station_manager.get_now_playing(station_id).await.ok();

        if let Some(np) = now_playing {
            let queued = QueuedTrack {
                track_id: np.track.id.clone(),
                title: np.track.title.clone(),
                artist: np.track.artist.clone(),
            };
            pipeline.queue_track(queued).await?;
            tracing::info!("Queued current track for station {} HLS stream", station.name);
        } else {
            tracing::warn!("No tracks available for station {} HLS stream", station.name);
        }
    }

    // Start the pipeline
    pipeline.start().await?;
    tracing::info!("Started audio pipeline for station {}", station.name);

    let pipeline_arc = Arc::new(pipeline);
    let broadcaster = Arc::new(AudioBroadcaster::new(
        pipeline_arc.clone(),
        AudioBroadcasterConfig::default(),
    ));

    // Store it
    {
        let mut broadcasters = state.station_broadcasters.write().await;
        broadcasters.insert(station_id, broadcaster.clone());
    }

    // Spawn a background task to keep the queue filled
    let state_clone = state.clone();
    let broadcaster_clone = broadcaster.clone();
    let pipeline_for_refill = pipeline_arc.clone();
    tokio::spawn(async move {
        let station_id = station_id;
        let mut last_queued_track_id: Option<String> = None;

        loop {
            // Check if broadcaster is still running
            if !broadcaster_clone.is_running() {
                tracing::debug!("Broadcaster stopped, ending refill task for station {}", station_id);
                break;
            }

            // Check queue length
            let queue_len = pipeline_for_refill.queue_length().await;

            // If queue is running low (less than 2 tracks), add more
            if queue_len < 2 {
                // Get next track from station manager
                match state_clone.station_manager.get_now_playing(station_id).await {
                    Ok(np) => {
                        // Only queue if it's a different track than last time
                        let track_id = np.track.id.clone();
                        if last_queued_track_id.as_ref() != Some(&track_id) {
                            let queued = QueuedTrack {
                                track_id: track_id.clone(),
                                title: np.track.title.clone(),
                                artist: np.track.artist.clone(),
                            };
                            if let Err(e) = pipeline_for_refill.queue_track(queued).await {
                                tracing::error!("Failed to queue track for station {}: {:?}", station_id, e);
                            } else {
                                tracing::debug!("Refilled queue with track: {} for station {}", np.track.title, station_id);
                                last_queued_track_id = Some(track_id);
                            }
                        }
                    }
                    Err(e) => {
                        tracing::debug!("Could not get now playing for refill: {:?}", e);
                    }
                }
            }

            // Wait before checking again
            tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
        }
        tracing::info!("Refill task ended for station {}", station_id);
    });

    Ok(broadcaster)
}

/// Get HLS playlist (m3u8) for a station
async fn get_hls_playlist(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<Response> {
    // Verify station exists
    let _station = sqlx::query_as::<_, Station>("SELECT * FROM stations WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound("Station not found".to_string()))?;

    let broadcaster = get_or_create_broadcaster(&state, id).await?;

    // Start broadcaster if not running
    if !broadcaster.is_running() {
        broadcaster.start().await?;
    }

    let playlist = broadcaster.get_playlist().await;

    let response = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/vnd.apple.mpegurl")
        .header(header::CACHE_CONTROL, "no-cache, no-store, must-revalidate")
        .body(Body::from(playlist))
        .map_err(|e| AppError::InternalMessage(format!("Failed to build response: {}", e)))?;

    Ok(response)
}

/// Get an HLS segment (audio chunk)
async fn get_hls_segment(
    State(state): State<Arc<AppState>>,
    Path((id, seq_str)): Path<(Uuid, String)>,
) -> Result<Response> {
    // Strip .mp3 extension if present
    let seq_clean = seq_str.trim_end_matches(".mp3");
    let seq: u64 = seq_clean
        .parse()
        .map_err(|_| AppError::Validation(format!("Invalid segment number: {}", seq_str)))?;

    let broadcaster = {
        let broadcasters = state.station_broadcasters.read().await;
        broadcasters
            .get(&id)
            .cloned()
            .ok_or_else(|| AppError::NotFound("Stream not found".to_string()))?
    };

    let segment = broadcaster
        .get_segment(seq)
        .await
        .ok_or_else(|| AppError::NotFound("Segment not found".to_string()))?;

    let response = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "audio/mpeg")
        .header(header::CACHE_CONTROL, "public, max-age=3600")
        .body(Body::from(segment.data))
        .map_err(|e| AppError::InternalMessage(format!("Failed to build response: {}", e)))?;

    Ok(response)
}

/// SSE endpoint for real-time visualization data
async fn visualization_sse(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<Sse<impl Stream<Item = std::result::Result<Event, Infallible>>>> {
    let broadcaster = {
        let broadcasters = state.station_broadcasters.read().await;
        broadcasters
            .get(&id)
            .cloned()
            .ok_or_else(|| AppError::NotFound("Stream not found".to_string()))?
    };

    let mut rx = broadcaster.subscribe_visualization();

    // Convert broadcast receiver to SSE stream
    let stream = async_stream::stream! {
        loop {
            match rx.recv().await {
                Ok(viz) => {
                    let data = serde_json::to_string(&viz).unwrap_or_else(|_| "{}".to_string());
                    yield Ok(Event::default().data(data));
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                    // Receiver fell behind, continue
                    continue;
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    // Channel closed, end stream
                    break;
                }
            }
        }
    };

    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}
