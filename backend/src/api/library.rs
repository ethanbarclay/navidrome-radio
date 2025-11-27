use crate::api::middleware::RequireAdmin;
use crate::api::stations::AppState;
use crate::error::{AppError, Result};
use crate::models::{LibraryStats, LibrarySyncStatus, SyncProgress};
use axum::{
    extract::{Path, State},
    response::sse::{Event, Sse},
    routing::{get, post},
    Json, Router,
};
use futures::stream::{Stream, StreamExt};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::convert::Infallible;
use tokio::sync::broadcast;

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
