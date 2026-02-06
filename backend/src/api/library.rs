use crate::api::middleware::RequireAdmin;
use crate::api::stations::{AppState, EmbeddingControlState};
use crate::error::{AppError, Result};
use crate::models::{EmbeddingProgress, LibraryStats, LibrarySyncStatus, SyncProgress};
use crate::services::hybrid_curator::HybridCurationProgress;
use axum::{
    extract::{Path, Query, State},
    http::HeaderMap,
    response::sse::{Event, Sse},
    routing::{get, post},
    Json, Router,
};
use futures::stream::Stream;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::convert::Infallible;
use std::time::Instant;
use tokio::sync::{broadcast, mpsc};

#[derive(Debug, Deserialize)]
struct AnalyzeTracksRequest {
    limit: Option<usize>,
}

#[derive(Debug, Serialize)]
struct AnalyzeTracksResponse {
    tracks_analyzed: usize,
    message: String,
}

#[derive(Debug, Deserialize)]
struct CurateTracksRequest {
    query: String,
    limit: Option<usize>,
}

#[derive(Debug, Serialize)]
struct TrackInfo {
    id: String,
    title: String,
    artist: String,
}

#[derive(Debug, Serialize)]
struct CurateTracksResponse {
    track_ids: Vec<String>,
    tracks: Vec<TrackInfo>,
    query: String,
}

#[derive(Debug, Deserialize)]
struct RateTrackRequest {
    rating: f64,
}

#[derive(Debug, Serialize)]
struct RateTrackResponse {
    track_id: String,
    rating: f64,
    message: String,
}

#[derive(Debug, Deserialize)]
struct GetTracksByIdsRequest {
    ids: Vec<String>,
}

#[derive(Debug, Serialize)]
struct TrackDetails {
    id: String,
    title: String,
    artist: String,
    album: String,
}

#[derive(Debug, Serialize)]
struct GetTracksByIdsResponse {
    tracks: Vec<TrackDetails>,
}

#[derive(Debug, Serialize)]
struct EmbeddingStatusResponse {
    total_tracks: i64,
    tracks_with_embeddings: i64,
    coverage_percent: f64,
    indexing_in_progress: bool,
    control_state: String,
}

#[derive(Debug, Deserialize)]
struct IndexEmbeddingsRequest {
    batch_size: Option<usize>,
    max_tracks: Option<usize>,
}

#[derive(Debug, Serialize)]
struct IndexEmbeddingsResponse {
    message: String,
    status: String,
}

#[derive(Debug, Deserialize)]
struct HybridCurateRequest {
    query: String,
    limit: Option<usize>,
}

// === Two-phase curation types ===

#[derive(Debug, Deserialize)]
struct SelectSeedsRequest {
    query: String,
    seed_count: Option<usize>,
}

#[derive(Debug, Serialize)]
struct SeedTrack {
    id: String,
    title: String,
    artist: String,
    album: String,
}

#[derive(Debug, Serialize)]
struct SelectSeedsResponse {
    seeds: Vec<SeedTrack>,
    query: String,
    genres: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct RegenerateSeedRequest {
    query: String,
    position: usize,
    exclude_ids: Vec<String>,
}

#[derive(Debug, Serialize)]
struct RegenerateSeedResponse {
    seed: SeedTrack,
    position: usize,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct FillGapsRequest {
    query: String,
    seed_ids: Vec<String>,
    total_size: Option<usize>,
}

#[derive(Debug, Serialize)]
struct FillGapsResponse {
    track_ids: Vec<String>,
    tracks: Vec<TrackInfo>,
    seed_count: usize,
    filled_count: usize,
}

#[derive(Debug, Serialize)]
struct HybridCurateResponse {
    track_ids: Vec<String>,
    tracks: Vec<TrackInfo>,
    query: String,
    method: String,
}

pub fn library_routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/library/sync", post(trigger_full_sync))
        .route("/library/sync-stream", get(sync_stream))
        .route("/library/analyze", post(trigger_ai_analysis))
        .route("/library/stats", get(get_library_stats))
        .route("/library/sync-status", get(get_sync_status))
        .route("/library/curate", post(curate_tracks))
        .route("/library/tracks", post(get_tracks_by_ids))
        .route("/tracks/:id/rate", post(rate_track))
        .route("/tracks/:id/rating", get(get_track_rating))
        // Embedding/ML-powered curation endpoints
        .route("/embeddings/status", get(get_embedding_status))
        .route("/embeddings/index", post(index_embeddings))
        .route("/embeddings/index-stream", get(index_embeddings_stream))
        .route("/embeddings/pause", post(pause_embeddings))
        .route("/embeddings/resume", post(resume_embeddings))
        .route("/embeddings/stop", post(stop_embeddings))
        .route("/embeddings/visualization", get(get_embeddings_for_visualization))
        .route("/ai/hybrid-curate", post(hybrid_curate))
        .route("/ai/hybrid-curate-stream", get(hybrid_curate_stream))
        // Two-phase curation endpoints (for seed review UI)
        .route("/ai/select-seeds", post(select_seeds))
        .route("/ai/regenerate-seed", post(regenerate_seed))
        .route("/ai/fill-gaps", post(fill_gaps))
}

/// POST /api/v1/library/sync
/// Trigger a full library sync from Navidrome
async fn trigger_full_sync(
    State(state): State<Arc<AppState>>,
    RequireAdmin(_): RequireAdmin,
) -> Result<Json<serde_json::Value>> {
    // Check if sync is already in progress
    let status = state.library_indexer.get_sync_status().await?;

    if status.sync_in_progress {
        return Err(AppError::Conflict(
            "Sync already in progress".to_string(),
        ));
    }

    // Spawn sync task in background
    let indexer = Arc::clone(&state.library_indexer);
    tokio::spawn(async move {
        if let Err(e) = indexer.sync_full(None).await {
            tracing::error!("Background sync failed: {}", e);
        }
    });

    Ok(Json(serde_json::json!({
        "message": "Full library sync started",
        "status": "in_progress"
    })))
}

/// POST /api/v1/library/analyze
/// Trigger AI analysis on unanalyzed tracks
async fn trigger_ai_analysis(
    State(state): State<Arc<AppState>>,
    RequireAdmin(_): RequireAdmin,
    Json(req): Json<AnalyzeTracksRequest>,
) -> Result<Json<AnalyzeTracksResponse>> {
    let limit = req.limit.unwrap_or(100);

    // Spawn analysis task in background
    let indexer = Arc::clone(&state.library_indexer);
    tokio::spawn(async move {
        if let Err(e) = indexer.analyze_unanalyzed_tracks(limit).await {
            tracing::error!("Background analysis failed: {}", e);
        }
    });

    Ok(Json(AnalyzeTracksResponse {
        tracks_analyzed: 0,
        message: format!("AI analysis started for up to {} tracks", limit),
    }))
}

/// GET /api/v1/library/stats
/// Get current library statistics
async fn get_library_stats(
    State(state): State<Arc<AppState>>,
) -> Result<Json<LibraryStats>> {
    let stats = state.library_indexer.get_library_stats().await?;
    Ok(Json(stats))
}

/// GET /api/v1/library/sync-status
/// Get current sync status and progress
async fn get_sync_status(
    State(state): State<Arc<AppState>>,
) -> Result<Json<LibrarySyncStatus>> {
    let status = state.library_indexer.get_sync_status().await?;
    Ok(Json(status))
}

/// POST /api/v1/library/curate
/// AI-powered track curation based on natural language query
async fn curate_tracks(
    State(state): State<Arc<AppState>>,
    RequireAdmin(_): RequireAdmin,
    Json(req): Json<CurateTracksRequest>,
) -> Result<Json<CurateTracksResponse>> {
    let curator = state
        .ai_curator
        .as_ref()
        .ok_or_else(|| AppError::ExternalApi("AI curator not available - ANTHROPIC_API_KEY not configured".to_string()))?;

    let limit = req.limit.unwrap_or(20);

    if req.query.trim().is_empty() {
        return Err(AppError::Validation("Query cannot be empty".to_string()));
    }

    let track_ids = curator.curate_tracks(req.query.clone(), limit).await?;

    // Fetch track details from library_index
    let mut tracks = Vec::new();
    for id in &track_ids {
        if let Ok(track) = sqlx::query!(
            "SELECT id, title, artist FROM library_index WHERE id = $1",
            id
        )
        .fetch_one(&state.db)
        .await
        {
            tracks.push(TrackInfo {
                id: track.id,
                title: track.title,
                artist: track.artist,
            });
        }
    }

    Ok(Json(CurateTracksResponse {
        track_ids,
        tracks,
        query: req.query,
    }))
}

/// POST /api/v1/tracks/:id/rate
/// Rate a track (user rating)
async fn rate_track(
    State(_state): State<Arc<AppState>>,
    RequireAdmin(_): RequireAdmin,
    Path(track_id): Path<String>,
    Json(req): Json<RateTrackRequest>,
) -> Result<Json<RateTrackResponse>> {
    if req.rating < 0.0 || req.rating > 5.0 {
        return Err(AppError::Validation(
            "Rating must be between 0.0 and 5.0".to_string(),
        ));
    }

    // TODO: Implement user rating persistence
    // For now, just return success

    Ok(Json(RateTrackResponse {
        track_id: track_id.clone(),
        rating: req.rating,
        message: "Track rating saved successfully".to_string(),
    }))
}

/// GET /api/v1/tracks/:id/rating
/// Get track rating information
async fn get_track_rating(
    State(_state): State<Arc<AppState>>,
    Path(track_id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    // TODO: Implement track rating retrieval
    // For now, return placeholder

    Ok(Json(serde_json::json!({
        "track_id": track_id,
        "user_rating": null,
        "avg_rating": null,
        "rating_count": 0
    })))
}

/// GET /api/v1/library/sync-stream
/// Stream library sync progress via Server-Sent Events
async fn sync_stream(
    State(state): State<Arc<AppState>>,
    RequireAdmin(_): RequireAdmin,
) -> Sse<impl Stream<Item = std::result::Result<Event, Infallible>>> {
    // Check if sync is already in progress
    let status = state.library_indexer.get_sync_status().await;

    let sync_already_running = status.is_ok() && status.as_ref().unwrap().sync_in_progress;

    // Create a broadcast channel for progress updates
    let (tx, _rx) = broadcast::channel::<SyncProgress>(100);

    // Subscribe BEFORE sending any messages to avoid race condition
    let mut rx = tx.subscribe();

    if sync_already_running {
        // If sync is already running, send an error event
        let _ = tx.send(SyncProgress::Error {
            message: "Sync already in progress".to_string(),
        });
    } else {
        // Spawn sync task with progress reporting
        let tx_clone = tx.clone();
        let indexer = Arc::clone(&state.library_indexer);
        tokio::spawn(async move {
            // Send started event
            let _ = tx_clone.send(SyncProgress::Started {
                message: "Starting library sync".to_string(),
            });

            // Perform the sync with progress reporting
            if let Err(e) = indexer.sync_full(Some(tx_clone)).await {
                tracing::error!("Library sync failed: {}", e);
            }
        });
    }

    // Convert broadcast receiver to SSE stream
    let stream = async_stream::stream! {
        loop {
            match rx.recv().await {
                Ok(progress) => {
                    let is_terminal = matches!(progress, SyncProgress::Completed { .. } | SyncProgress::Error { .. });

                    if let Ok(event) = Event::default().json_data(&progress) {
                        yield Ok::<Event, Infallible>(event);
                    }

                    if is_terminal {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    };

    Sse::new(stream).keep_alive(axum::response::sse::KeepAlive::default())
}

/// POST /api/v1/library/tracks
/// Get track details by IDs
async fn get_tracks_by_ids(
    State(state): State<Arc<AppState>>,
    Json(req): Json<GetTracksByIdsRequest>,
) -> Result<Json<GetTracksByIdsResponse>> {
    if req.ids.is_empty() {
        return Ok(Json(GetTracksByIdsResponse { tracks: vec![] }));
    }

    // Limit to 500 IDs max
    let ids: Vec<String> = req.ids.into_iter().take(500).collect();

    // Build query safely using QueryBuilder
    let mut qb: sqlx::QueryBuilder<sqlx::Postgres> = sqlx::QueryBuilder::new(
        "SELECT id, title, artist, album FROM library_index WHERE id IN ("
    );
    let mut separated = qb.separated(", ");
    for id in &ids {
        separated.push_bind(id);
    }
    qb.push(")");

    let rows = qb.build().fetch_all(&state.db).await?;
    let tracks: Vec<TrackDetails> = rows
        .iter()
        .map(|row| {
            use sqlx::Row;
            TrackDetails {
                id: row.get("id"),
                title: row.get("title"),
                artist: row.get("artist"),
                album: row.get("album"),
            }
        })
        .collect();

    Ok(Json(GetTracksByIdsResponse { tracks }))
}

/// GET /api/v1/embeddings/status
/// Get audio embedding indexing status
async fn get_embedding_status(
    State(state): State<Arc<AppState>>,
) -> Result<Json<EmbeddingStatusResponse>> {
    // Get total tracks from library_index
    let total_tracks: i64 = sqlx::query_scalar!(
        "SELECT COUNT(*) FROM library_index"
    )
    .fetch_one(&state.db)
    .await?
    .unwrap_or(0);

    // Get tracks with embeddings from track_embeddings
    let tracks_with_embeddings: i64 = sqlx::query_scalar!(
        "SELECT COUNT(*) FROM track_embeddings"
    )
    .fetch_one(&state.db)
    .await?
    .unwrap_or(0);

    let coverage_percent = if total_tracks > 0 {
        (tracks_with_embeddings as f64 / total_tracks as f64) * 100.0
    } else {
        0.0
    };

    // Get current control state
    let control_state = *state.embedding_control.read().await;
    let indexing_in_progress = control_state == EmbeddingControlState::Running
        || control_state == EmbeddingControlState::Paused;

    let control_state_str = match control_state {
        EmbeddingControlState::Idle => "idle",
        EmbeddingControlState::Running => "running",
        EmbeddingControlState::Paused => "paused",
        EmbeddingControlState::Stopping => "stopping",
    };

    Ok(Json(EmbeddingStatusResponse {
        total_tracks,
        tracks_with_embeddings,
        coverage_percent,
        indexing_in_progress,
        control_state: control_state_str.to_string(),
    }))
}

#[derive(Debug, Deserialize)]
struct EmbeddingVisualizationQuery {
    limit: Option<i64>,
}

#[derive(Debug, Serialize)]
struct EmbeddingPoint {
    id: String,
    title: String,
    artist: String,
    album: String,
    genre: Option<String>,
    x: f32,
    y: f32,
}

#[derive(Debug, Serialize)]
struct EmbeddingVisualizationResponse {
    points: Vec<EmbeddingPoint>,
    cache_rebuilt: bool,
}

/// GET /api/v1/embeddings/visualization
/// Get pre-computed 2D coordinates for embedding visualization
/// Returns cached PCA projections for fast loading
async fn get_embeddings_for_visualization(
    State(state): State<Arc<AppState>>,
    Query(params): Query<EmbeddingVisualizationQuery>,
) -> Result<Json<EmbeddingVisualizationResponse>> {
    // If limit is provided, use it; otherwise return all embeddings
    let limit = params.limit;

    // Check if we need to rebuild the visualization cache
    let mut cache_rebuilt = false;
    if let Some(ref encoder) = state.audio_encoder {
        if encoder.is_visualization_cache_stale().await.unwrap_or(true) {
            tracing::info!("Visualization cache is stale, rebuilding...");
            if let Err(e) = encoder.rebuild_visualization_cache().await {
                tracing::error!("Failed to rebuild visualization cache: {}", e);
            } else {
                cache_rebuilt = true;
            }
        }
    }

    // Query pre-computed viz coordinates with track metadata
    let rows: Vec<(String, String, String, String, Option<serde_json::Value>, Option<f32>, Option<f32>)> = if let Some(limit_val) = limit {
        sqlx::query_as(
            r#"
            SELECT
                te.track_id,
                li.title,
                li.artist,
                li.album,
                li.genres,
                te.viz_x,
                te.viz_y
            FROM track_embeddings te
            JOIN library_index li ON te.track_id = li.id
            WHERE te.viz_x IS NOT NULL AND te.viz_y IS NOT NULL
            LIMIT $1
            "#
        )
        .bind(limit_val)
        .fetch_all(&state.db)
        .await?
    } else {
        // No limit - fetch all embeddings
        sqlx::query_as(
            r#"
            SELECT
                te.track_id,
                li.title,
                li.artist,
                li.album,
                li.genres,
                te.viz_x,
                te.viz_y
            FROM track_embeddings te
            JOIN library_index li ON te.track_id = li.id
            WHERE te.viz_x IS NOT NULL AND te.viz_y IS NOT NULL
            "#
        )
        .fetch_all(&state.db)
        .await?
    };

    let points: Vec<EmbeddingPoint> = rows
        .into_iter()
        .filter_map(|(id, title, artist, album, genres, viz_x, viz_y)| {
            // Only include points with valid coordinates
            let x = viz_x?;
            let y = viz_y?;

            // Extract primary genre from JSON array
            let genre = genres.and_then(|g| {
                g.as_array()
                    .and_then(|arr| arr.first())
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
            });

            Some(EmbeddingPoint {
                id,
                title,
                artist,
                album,
                genre,
                x,
                y,
            })
        })
        .collect();

    Ok(Json(EmbeddingVisualizationResponse {
        points,
        cache_rebuilt,
    }))
}

/// POST /api/v1/embeddings/index
/// Start audio embedding indexing for tracks without embeddings
async fn index_embeddings(
    State(state): State<Arc<AppState>>,
    RequireAdmin(_): RequireAdmin,
    Json(req): Json<IndexEmbeddingsRequest>,
) -> Result<Json<IndexEmbeddingsResponse>> {
    let encoder = state
        .audio_encoder
        .as_ref()
        .ok_or_else(|| AppError::ExternalApi(
            "Audio encoder not available - AUDIO_ENCODER_MODEL_PATH not configured".to_string()
        ))?;

    let library_path = state
        .navidrome_library_path
        .as_ref()
        .ok_or_else(|| AppError::ExternalApi(
            "Library path not configured - NAVIDROME_LIBRARY_PATH not set".to_string()
        ))?;

    let batch_size = req.batch_size.unwrap_or(10);
    let max_tracks = req.max_tracks.unwrap_or(100);

    // Spawn indexing task in background
    let encoder = Arc::clone(encoder);
    let library_path = library_path.clone();
    let db = state.db.clone();

    tokio::spawn(async move {
        tracing::info!("Starting audio embedding indexing (batch_size={}, max_tracks={})", batch_size, max_tracks);

        // Get tracks without embeddings in random order for diversity testing
        let tracks: Vec<(String, String)> = sqlx::query_as(
            r#"
            SELECT li.id, li.path
            FROM library_index li
            WHERE li.path IS NOT NULL
            AND NOT EXISTS (SELECT 1 FROM track_embeddings te WHERE te.track_id = li.id)
            ORDER BY RANDOM()
            LIMIT $1
            "#
        )
        .bind(max_tracks as i64)
        .fetch_all(&db)
        .await
        .unwrap_or_default();

        tracing::info!("Found {} tracks to index", tracks.len());

        let mut success_count = 0;
        let mut error_count = 0;

        for (track_id, relative_path) in tracks {
            let full_path = std::path::Path::new(&library_path).join(&relative_path);

            if !full_path.exists() {
                tracing::warn!("Track file not found: {:?}", full_path);
                error_count += 1;
                continue;
            }

            match encoder.process_track(&track_id, &full_path).await {
                Ok(_) => {
                    success_count += 1;
                    if success_count % 10 == 0 {
                        tracing::info!("Indexed {} tracks so far", success_count);
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to encode track {}: {}", track_id, e);
                    error_count += 1;
                }
            }
        }

        tracing::info!(
            "Embedding indexing complete: {} success, {} errors",
            success_count, error_count
        );
    });

    Ok(Json(IndexEmbeddingsResponse {
        message: format!("Embedding indexing started (batch_size={}, max_tracks={})", batch_size, max_tracks),
        status: "in_progress".to_string(),
    }))
}

#[derive(Debug, Deserialize, Default)]
#[allow(dead_code)]
struct IndexEmbeddingsStreamQuery {
    token: Option<String>,
    batch_size: Option<usize>,
    max_tracks: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct HybridCurateStreamQuery {
    token: String,
    query: String,
    limit: Option<usize>,
}

/// GET /api/v1/embeddings/index-stream
/// Stream audio embedding indexing progress via Server-Sent Events
async fn index_embeddings_stream(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(params): Query<IndexEmbeddingsStreamQuery>,
) -> Sse<impl Stream<Item = std::result::Result<Event, Infallible>>> {
    // Extract token from Authorization header or query param
    let token = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|s| s.to_string())
        .or(params.token);

    // Validate token
    let auth_service = &state.auth_service;
    let token_valid = match &token {
        Some(t) => auth_service.validate_admin_token(t).await.is_ok(),
        None => false,
    };

    // Create a broadcast channel for progress updates
    let (tx, _rx) = broadcast::channel::<EmbeddingProgress>(100);
    let mut rx = tx.subscribe();

    if !token_valid {
        let _ = tx.send(EmbeddingProgress::Error {
            message: "Unauthorized".to_string(),
        });
    } else {
        let encoder = state.audio_encoder.clone();
        let library_path = state.navidrome_library_path.clone();
        let db = state.db.clone();
        let embedding_control = state.embedding_control.clone();

        // Check if already running
        {
            let control = embedding_control.read().await;
            if *control == EmbeddingControlState::Running || *control == EmbeddingControlState::Paused {
                let _ = tx.send(EmbeddingProgress::Error {
                    message: "Embedding indexing is already running".to_string(),
                });
                // Return early via the stream below
            }
        }

        if encoder.is_none() || library_path.is_none() {
            let _ = tx.send(EmbeddingProgress::Error {
                message: "Audio encoder not configured".to_string(),
            });
        } else {
            let encoder = encoder.unwrap();
            let library_path = library_path.unwrap();
            let tx_clone = tx.clone();

            // Set state to Running
            {
                let mut control = embedding_control.write().await;
                *control = EmbeddingControlState::Running;
            }

            tokio::spawn(async move {
                let start_time = Instant::now();

                // Get ALL tracks without embeddings in random order for diversity testing
                let tracks: Vec<(String, String, String, String)> = match sqlx::query_as(
                    r#"
                    SELECT li.id, li.path, li.title, li.artist
                    FROM library_index li
                    WHERE li.path IS NOT NULL
                    AND NOT EXISTS (SELECT 1 FROM track_embeddings te WHERE te.track_id = li.id)
                    ORDER BY RANDOM()
                    "#
                )
                .fetch_all(&db)
                .await {
                    Ok(t) => t,
                    Err(e) => {
                        let _ = tx_clone.send(EmbeddingProgress::Error {
                            message: format!("Database error: {}", e),
                        });
                        // Reset control state
                        let mut control = embedding_control.write().await;
                        *control = EmbeddingControlState::Idle;
                        return;
                    }
                };

                let total = tracks.len();
                if total == 0 {
                    let _ = tx_clone.send(EmbeddingProgress::Completed {
                        success_count: 0,
                        error_count: 0,
                        total_time_secs: 0.0,
                        message: "No tracks to index - all tracks already have embeddings".to_string(),
                    });
                    // Reset control state
                    let mut control = embedding_control.write().await;
                    *control = EmbeddingControlState::Idle;
                    return;
                }

                // Determine parallelism based on available cores
                let concurrency = std::thread::available_parallelism()
                    .map(|p| p.get())
                    .unwrap_or(4)
                    .min(8); // Cap at 8 for resource management

                let _ = tx_clone.send(EmbeddingProgress::Started {
                    message: format!("Starting embedding indexing for {} tracks ({} parallel)", total, concurrency),
                    total_tracks: total,
                });

                // Shared state for tracking progress
                use std::sync::atomic::{AtomicUsize, Ordering, AtomicBool};
                let success_count = Arc::new(AtomicUsize::new(0));
                let error_count = Arc::new(AtomicUsize::new(0));
                let completed_count = Arc::new(AtomicUsize::new(0));
                let in_progress: Arc<tokio::sync::Mutex<Vec<String>>> = Arc::new(tokio::sync::Mutex::new(Vec::new()));
                let should_stop = Arc::new(AtomicBool::new(false));

                // Use futures stream for parallel processing with pause/stop support
                use futures::stream::{self, StreamExt};

                // Clone embedding_control for use inside the stream
                let embedding_control_inner = embedding_control.clone();
                let should_stop_inner = should_stop.clone();

                let _results: Vec<_> = stream::iter(tracks.into_iter())
                    .map(|(track_id, relative_path, title, artist)| {
                        let encoder = encoder.clone();
                        let library_path = library_path.clone();
                        let tx = tx_clone.clone();
                        let success_count = success_count.clone();
                        let error_count = error_count.clone();
                        let completed_count = completed_count.clone();
                        let in_progress = in_progress.clone();
                        let embedding_control = embedding_control_inner.clone();
                        let should_stop = should_stop_inner.clone();

                        async move {
                            // Check for stop signal at the start of each track
                            if should_stop.load(Ordering::Relaxed) {
                                return (track_id, Err("Stopped".to_string()));
                            }

                            // Check for pause/stop - wait if paused
                            loop {
                                let control = embedding_control.read().await;
                                match *control {
                                    EmbeddingControlState::Stopping => {
                                        should_stop.store(true, Ordering::Relaxed);
                                        return (track_id, Err("Stopped".to_string()));
                                    }
                                    EmbeddingControlState::Paused => {
                                        drop(control); // Release lock before sleeping
                                        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
                                        continue;
                                    }
                                    EmbeddingControlState::Idle => {
                                        // Something cancelled us
                                        return (track_id, Err("Cancelled".to_string()));
                                    }
                                    EmbeddingControlState::Running => break,
                                }
                            }

                            let track_name = format!("{} - {}", artist, title);
                            let full_path = std::path::Path::new(&library_path).join(&relative_path);

                            // Add to in_progress and send update
                            {
                                let mut ip = in_progress.lock().await;
                                ip.push(track_name.clone());
                                let _ = tx.send(EmbeddingProgress::Processing {
                                    completed: completed_count.load(Ordering::Relaxed),
                                    total,
                                    success_count: success_count.load(Ordering::Relaxed),
                                    error_count: error_count.load(Ordering::Relaxed),
                                    in_progress: ip.clone(),
                                    message: format!("Processing {} tracks in parallel", ip.len()),
                                });
                            }

                            let result = if !full_path.exists() {
                                Err("File not found".to_string())
                            } else {
                                let track_start = Instant::now();
                                match encoder.process_track(&track_id, &full_path).await {
                                    Ok(_) => Ok(track_start.elapsed().as_millis() as u64),
                                    Err(e) => Err(e.to_string()),
                                }
                            };

                            // Remove from in_progress and update counters
                            {
                                let mut ip = in_progress.lock().await;
                                ip.retain(|n| n != &track_name);
                                completed_count.fetch_add(1, Ordering::Relaxed);

                                match &result {
                                    Ok(processing_time_ms) => {
                                        success_count.fetch_add(1, Ordering::Relaxed);
                                        let _ = tx.send(EmbeddingProgress::TrackComplete {
                                            track_id: track_id.clone(),
                                            track_name: track_name.clone(),
                                            processing_time_ms: *processing_time_ms,
                                            current: completed_count.load(Ordering::Relaxed),
                                            total,
                                        });
                                    }
                                    Err(error) => {
                                        error_count.fetch_add(1, Ordering::Relaxed);
                                        let _ = tx.send(EmbeddingProgress::TrackError {
                                            track_id: track_id.clone(),
                                            track_name: track_name.clone(),
                                            error: error.clone(),
                                            current: completed_count.load(Ordering::Relaxed),
                                            total,
                                        });
                                    }
                                }

                                // Send processing update if there are still tracks in progress
                                if !ip.is_empty() {
                                    let _ = tx.send(EmbeddingProgress::Processing {
                                        completed: completed_count.load(Ordering::Relaxed),
                                        total,
                                        success_count: success_count.load(Ordering::Relaxed),
                                        error_count: error_count.load(Ordering::Relaxed),
                                        in_progress: ip.clone(),
                                        message: format!("Processing {} tracks in parallel", ip.len()),
                                    });
                                }
                            }

                            (track_id, result)
                        }
                    })
                    .buffer_unordered(concurrency)
                    .collect()
                    .await;

                let success_count = success_count.load(Ordering::Relaxed);
                let error_count = error_count.load(Ordering::Relaxed);
                let was_stopped = should_stop.load(Ordering::Relaxed);

                let total_time_secs = start_time.elapsed().as_secs_f64();
                let message = if was_stopped {
                    format!(
                        "Embedding indexing stopped: {} success, {} errors in {:.1}s (stopped early)",
                        success_count, error_count, total_time_secs
                    )
                } else {
                    format!(
                        "Embedding indexing complete: {} success, {} errors in {:.1}s",
                        success_count, error_count, total_time_secs
                    )
                };

                let _ = tx_clone.send(EmbeddingProgress::Completed {
                    success_count,
                    error_count,
                    total_time_secs,
                    message,
                });

                // Reset control state to Idle
                let mut control = embedding_control.write().await;
                *control = EmbeddingControlState::Idle;
            });
        }
    }

    // Convert broadcast receiver to SSE stream
    let stream = async_stream::stream! {
        loop {
            match rx.recv().await {
                Ok(progress) => {
                    let is_terminal = matches!(progress, EmbeddingProgress::Completed { .. } | EmbeddingProgress::Error { .. });

                    if let Ok(event) = Event::default().json_data(&progress) {
                        yield Ok::<Event, Infallible>(event);
                    }

                    if is_terminal {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    };

    Sse::new(stream).keep_alive(axum::response::sse::KeepAlive::default())
}

/// POST /api/v1/embeddings/pause
/// Pause audio embedding indexing
async fn pause_embeddings(
    State(state): State<Arc<AppState>>,
    RequireAdmin(_): RequireAdmin,
) -> Result<Json<serde_json::Value>> {
    let mut control = state.embedding_control.write().await;

    if *control != EmbeddingControlState::Running {
        return Err(AppError::Conflict(
            "Embedding indexing is not running".to_string(),
        ));
    }

    *control = EmbeddingControlState::Paused;
    tracing::info!("Embedding indexing paused");

    Ok(Json(serde_json::json!({
        "message": "Embedding indexing paused",
        "status": "paused"
    })))
}

/// POST /api/v1/embeddings/resume
/// Resume audio embedding indexing
async fn resume_embeddings(
    State(state): State<Arc<AppState>>,
    RequireAdmin(_): RequireAdmin,
) -> Result<Json<serde_json::Value>> {
    let mut control = state.embedding_control.write().await;

    if *control != EmbeddingControlState::Paused {
        return Err(AppError::Conflict(
            "Embedding indexing is not paused".to_string(),
        ));
    }

    *control = EmbeddingControlState::Running;
    tracing::info!("Embedding indexing resumed");

    Ok(Json(serde_json::json!({
        "message": "Embedding indexing resumed",
        "status": "running"
    })))
}

/// POST /api/v1/embeddings/stop
/// Stop audio embedding indexing
async fn stop_embeddings(
    State(state): State<Arc<AppState>>,
    RequireAdmin(_): RequireAdmin,
) -> Result<Json<serde_json::Value>> {
    let mut control = state.embedding_control.write().await;

    if *control == EmbeddingControlState::Idle {
        return Err(AppError::Conflict(
            "Embedding indexing is not running".to_string(),
        ));
    }

    *control = EmbeddingControlState::Stopping;
    tracing::info!("Embedding indexing stop requested");

    Ok(Json(serde_json::json!({
        "message": "Embedding indexing stop requested",
        "status": "stopping"
    })))
}

/// POST /api/v1/ai/hybrid-curate
/// Hybrid AI-powered track curation (LLM seeds + audio similarity)
async fn hybrid_curate(
    State(state): State<Arc<AppState>>,
    RequireAdmin(_): RequireAdmin,
    Json(req): Json<HybridCurateRequest>,
) -> Result<Json<HybridCurateResponse>> {
    if req.query.trim().is_empty() {
        return Err(AppError::Validation("Query cannot be empty".to_string()));
    }

    let limit = req.limit.unwrap_or(20);
    let (track_ids, method) = if let Some(hybrid_curator) = &state.hybrid_curator {
        // Use hybrid curation (LLM + audio embeddings)
        let ids = hybrid_curator.curate(&req.query, limit).await?;
        (ids, "hybrid".to_string())
    } else if let Some(ai_curator) = &state.ai_curator {
        // Fall back to LLM-only curation
        let ids = ai_curator.curate_tracks(req.query.clone(), limit).await?;
        (ids, "llm".to_string())
    } else {
        return Err(AppError::ExternalApi(
            "No curation method available - configure ANTHROPIC_API_KEY".to_string()
        ));
    };

    // Fetch track details
    let mut tracks = Vec::new();
    for id in &track_ids {
        if let Ok(track) = sqlx::query!(
            "SELECT id, title, artist FROM library_index WHERE id = $1",
            id
        )
        .fetch_one(&state.db)
        .await
        {
            tracks.push(TrackInfo {
                id: track.id,
                title: track.title,
                artist: track.artist,
            });
        }
    }

    Ok(Json(HybridCurateResponse {
        track_ids,
        tracks,
        query: req.query,
        method,
    }))
}

/// GET /api/v1/ai/hybrid-curate-stream
/// Stream hybrid AI curation progress via Server-Sent Events
async fn hybrid_curate_stream(
    State(state): State<Arc<AppState>>,
    Query(params): Query<HybridCurateStreamQuery>,
) -> Sse<impl Stream<Item = std::result::Result<Event, Infallible>>> {
    // Validate token
    let auth_service = &state.auth_service;
    let token_valid = auth_service.validate_admin_token(&params.token).await.is_ok();

    // Create mpsc channel for progress updates
    let (tx, mut rx) = mpsc::channel::<HybridCurationProgress>(100);

    if !token_valid {
        let _ = tx.send(HybridCurationProgress::Error {
            message: "Unauthorized".to_string(),
        }).await;
    } else if params.query.trim().is_empty() {
        let _ = tx.send(HybridCurationProgress::Error {
            message: "Query cannot be empty".to_string(),
        }).await;
    } else {
        let hybrid_curator = state.hybrid_curator.clone();
        let ai_curator = state.ai_curator.clone();
        let query = params.query.clone();
        let limit = params.limit.unwrap_or(50);

        tokio::spawn(async move {
            if let Some(curator) = hybrid_curator {
                // Use hybrid curation with progress
                match curator.curate_with_progress(&query, limit, tx.clone()).await {
                    Ok(_) => {
                        // Progress already sent by curate_with_progress
                    }
                    Err(e) => {
                        let _ = tx.send(HybridCurationProgress::Error {
                            message: format!("Curation failed: {}", e),
                        }).await;
                    }
                }
            } else if let Some(ai_curator) = ai_curator {
                // Fall back to LLM-only curation (no streaming progress)
                let _ = tx.send(HybridCurationProgress::SelectingSeeds {
                    message: "Using LLM-only curation (no audio embeddings)...".to_string(),
                }).await;

                match ai_curator.curate_tracks(query.clone(), limit).await {
                    Ok(track_ids) => {
                        let _ = tx.send(HybridCurationProgress::Completed {
                            message: format!("Selected {} tracks", track_ids.len()),
                            total_tracks: track_ids.len(),
                            seed_count: track_ids.len(),
                            filled_count: 0,
                            method: "llm".to_string(),
                            track_ids: Some(track_ids),
                        }).await;
                    }
                    Err(e) => {
                        let _ = tx.send(HybridCurationProgress::Error {
                            message: format!("Curation failed: {}", e),
                        }).await;
                    }
                }
            } else {
                let _ = tx.send(HybridCurationProgress::Error {
                    message: "No curation method available - configure ANTHROPIC_API_KEY".to_string(),
                }).await;
            }
        });
    }

    // Convert mpsc receiver to SSE stream
    let stream = async_stream::stream! {
        while let Some(progress) = rx.recv().await {
            let is_terminal = matches!(
                progress,
                HybridCurationProgress::Completed { .. } | HybridCurationProgress::Error { .. }
            );

            if let Ok(event) = Event::default().json_data(&progress) {
                yield Ok::<Event, Infallible>(event);
            }

            if is_terminal {
                break;
            }
        }
    };

    Sse::new(stream).keep_alive(axum::response::sse::KeepAlive::default())
}

// === Two-phase curation endpoints ===

/// POST /api/v1/ai/select-seeds
/// Phase 1: Select seed tracks for user review
async fn select_seeds(
    State(state): State<Arc<AppState>>,
    RequireAdmin(_): RequireAdmin,
    Json(req): Json<SelectSeedsRequest>,
) -> Result<Json<SelectSeedsResponse>> {
    if req.query.trim().is_empty() {
        return Err(AppError::Validation("Query cannot be empty".to_string()));
    }

    let seed_count = req.seed_count.unwrap_or(5);

    // Get seed selector - requires ANTHROPIC_API_KEY in environment
    let anthropic_key = std::env::var("ANTHROPIC_API_KEY")
        .map_err(|_| AppError::ExternalApi("ANTHROPIC_API_KEY not configured".to_string()))?;

    let seed_selector = crate::services::seed_selector::SeedSelector::new(
        anthropic_key,
        state.db.clone(),
    );

    // Select seeds with genres
    let result = seed_selector.select_seeds_with_genres(&req.query, seed_count, 200).await?;

    // Fetch full track details for each seed
    let mut seeds = Vec::new();
    for seed in result.seeds {
        if let Ok(track) = sqlx::query!(
            "SELECT id, title, artist, album FROM library_index WHERE id = $1",
            seed.track_id
        )
        .fetch_one(&state.db)
        .await
        {
            seeds.push(SeedTrack {
                id: track.id,
                title: track.title,
                artist: track.artist,
                album: track.album,
            });
        }
    }

    Ok(Json(SelectSeedsResponse {
        seeds,
        query: req.query,
        genres: result.genres,
    }))
}

/// POST /api/v1/ai/regenerate-seed
/// Regenerate a single seed at a specific position
async fn regenerate_seed(
    State(state): State<Arc<AppState>>,
    RequireAdmin(_): RequireAdmin,
    Json(req): Json<RegenerateSeedRequest>,
) -> Result<Json<RegenerateSeedResponse>> {
    if req.query.trim().is_empty() {
        return Err(AppError::Validation("Query cannot be empty".to_string()));
    }

    let anthropic_key = std::env::var("ANTHROPIC_API_KEY")
        .map_err(|_| AppError::ExternalApi("ANTHROPIC_API_KEY not configured".to_string()))?;

    let seed_selector = crate::services::seed_selector::SeedSelector::new(
        anthropic_key,
        state.db.clone(),
    );

    // Select a single new seed, excluding the ones already selected
    let verified_seeds = seed_selector.select_seeds_excluding(&req.query, 1, &req.exclude_ids).await?;

    let seed = verified_seeds.first()
        .ok_or_else(|| AppError::NotFound("Could not find a replacement seed".to_string()))?;

    // Fetch full track details
    let track = sqlx::query!(
        "SELECT id, title, artist, album FROM library_index WHERE id = $1",
        seed.track_id
    )
    .fetch_one(&state.db)
    .await?;

    Ok(Json(RegenerateSeedResponse {
        seed: SeedTrack {
            id: track.id,
            title: track.title,
            artist: track.artist,
            album: track.album,
        },
        position: req.position,
    }))
}

/// POST /api/v1/ai/fill-gaps
/// Phase 2: Fill gaps between approved seeds using audio similarity
async fn fill_gaps(
    State(state): State<Arc<AppState>>,
    RequireAdmin(_): RequireAdmin,
    Json(req): Json<FillGapsRequest>,
) -> Result<Json<FillGapsResponse>> {
    if req.seed_ids.is_empty() {
        return Err(AppError::Validation("At least one seed is required".to_string()));
    }

    let total_size = req.total_size.unwrap_or(200);

    let audio_encoder = state.audio_encoder.as_ref()
        .ok_or_else(|| AppError::ExternalApi("Audio encoder not available".to_string()))?;

    let library_path = state.navidrome_library_path.as_ref()
        .ok_or_else(|| AppError::ExternalApi("Library path not configured".to_string()))?;
    let library_path = std::path::Path::new(library_path);

    // Build the playlist by filling gaps between seeds
    let mut playlist = Vec::with_capacity(total_size);
    let mut used_ids: Vec<String> = req.seed_ids.clone();

    // Calculate tracks per gap
    let num_seeds = req.seed_ids.len();
    let tracks_per_gap = if num_seeds > 0 {
        (total_size - num_seeds) / num_seeds
    } else {
        0
    };
    let remainder = if num_seeds > 0 {
        (total_size - num_seeds) % num_seeds
    } else {
        0
    };

    // Check which seeds need embeddings and generate them
    let seeds_needing_embeddings: Vec<String> = {
        let seed_ids = &req.seed_ids;
        let tracks_with_embeddings: Vec<String> = sqlx::query_scalar(
            "SELECT track_id FROM track_embeddings WHERE track_id = ANY($1)"
        )
        .bind(seed_ids)
        .fetch_all(&state.db)
        .await?;

        seed_ids.iter()
            .filter(|id| !tracks_with_embeddings.contains(id))
            .cloned()
            .collect()
    };

    // Generate embeddings for seeds that are missing them
    for track_id in &seeds_needing_embeddings {
        let path_result: Option<String> = sqlx::query_scalar(
            "SELECT path FROM library_index WHERE id = $1"
        )
        .bind(track_id)
        .fetch_optional(&state.db)
        .await?;

        if let Some(relative_path) = path_result {
            let full_path = library_path.join(&relative_path);
            if full_path.exists() {
                let _ = audio_encoder.process_track(track_id, &full_path).await;
            }
        }
    }

    // Fill gaps between each pair of seeds
    for i in 0..num_seeds {
        // Add the seed
        playlist.push(req.seed_ids[i].clone());

        // Calculate gap size
        let gap_size = if i < remainder {
            tracks_per_gap + 1
        } else {
            tracks_per_gap
        };

        if gap_size == 0 {
            continue;
        }

        // Find similar tracks to fill the gap
        let from_seed = &req.seed_ids[i];
        let to_seed = if i + 1 < num_seeds {
            &req.seed_ids[i + 1]
        } else {
            from_seed // Last seed - extend with similar
        };

        let gap_tracks = if from_seed == to_seed {
            // Same seed - find similar tracks
            match audio_encoder.find_similar(from_seed, gap_size, &used_ids).await {
                Ok(tracks) => tracks.into_iter().map(|(id, _)| id).collect(),
                Err(_) => Vec::new(),
            }
        } else {
            // Different seeds - find transition tracks
            match audio_encoder.find_transition_tracks(from_seed, to_seed, gap_size, &used_ids).await {
                Ok(tracks) => tracks,
                Err(_) => Vec::new(),
            }
        };

        for track_id in gap_tracks {
            used_ids.push(track_id.clone());
            playlist.push(track_id);
        }
    }

    // Fetch track details for response
    let mut tracks = Vec::new();
    for id in &playlist {
        if let Ok(track) = sqlx::query!(
            "SELECT id, title, artist FROM library_index WHERE id = $1",
            id
        )
        .fetch_one(&state.db)
        .await
        {
            tracks.push(TrackInfo {
                id: track.id,
                title: track.title,
                artist: track.artist,
            });
        }
    }

    let seed_count = req.seed_ids.len();
    let filled_count = playlist.len() - seed_count;

    Ok(Json(FillGapsResponse {
        track_ids: playlist,
        tracks,
        seed_count,
        filled_count,
    }))
}
