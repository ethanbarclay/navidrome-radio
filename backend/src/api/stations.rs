use crate::api::middleware::{RequireAdmin, RequireAuth};
use crate::error::{AppError, Result};
use crate::models::{CreateStationRequest, NowPlaying, Station, UpdateStationRequest};
use crate::services::{AuthService, CurationEngine, StationManager};
use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::sync::Arc;
use uuid::Uuid;
use validator::Validate;

pub struct AppState {
    pub db: PgPool,
    pub auth_service: Arc<AuthService>,
    pub station_manager: Arc<StationManager>,
    pub curation_engine: Arc<CurationEngine>,
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
        .route("/stations/:id", get(get_station).patch(update_station).delete(delete_station))
        .route("/stations/:id/start", post(start_station))
        .route("/stations/:id/stop", post(stop_station))
        .route("/stations/:id/skip", post(skip_track))
        .route("/stations/:id/nowplaying", get(now_playing))
        .route("/ai/capabilities", get(ai_capabilities))
        .route("/ai/analyze-description", post(analyze_description))
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

    // Check if path is unique
    let exists = sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM stations WHERE path = $1)")
        .bind(&req.path)
        .fetch_one(&state.db)
        .await?;

    if exists {
        return Err(AppError::Validation("Station path already exists".to_string()));
    }

    let config = req.config.unwrap_or_default();

    let station = sqlx::query_as::<_, Station>(
        r#"
        INSERT INTO stations (path, name, description, genres, mood_tags, created_by, config)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
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
