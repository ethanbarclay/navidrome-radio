use crate::api::middleware::RequireAdmin;
use crate::error::Result;
use crate::AppState;
use axum::{
    extract::State,
    routing::{get, put},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Serialize, Deserialize)]
pub struct AppSettings {
    pub site_title: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateSettingsRequest {
    pub site_title: Option<String>,
}

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/", get(get_settings))
        .route("/", put(update_settings))
}

/// Get application settings (public)
async fn get_settings(State(state): State<Arc<AppState>>) -> Result<Json<AppSettings>> {
    let site_title: String = sqlx::query_scalar(
        "SELECT value FROM app_settings WHERE key = 'site_title'"
    )
    .fetch_optional(&state.db)
    .await?
    .unwrap_or_else(|| "NAVIDROME RADIO".to_string());

    Ok(Json(AppSettings { site_title }))
}

/// Update application settings (admin only)
async fn update_settings(
    State(state): State<Arc<AppState>>,
    RequireAdmin(_): RequireAdmin,
    Json(req): Json<UpdateSettingsRequest>,
) -> Result<Json<AppSettings>> {
    if let Some(title) = &req.site_title {
        sqlx::query(
            "INSERT INTO app_settings (key, value, updated_at) VALUES ('site_title', $1, NOW())
             ON CONFLICT (key) DO UPDATE SET value = $1, updated_at = NOW()"
        )
        .bind(title)
        .execute(&state.db)
        .await?;
    }

    // Return updated settings
    get_settings(State(state)).await
}
