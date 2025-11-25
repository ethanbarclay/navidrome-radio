use crate::api::stations::AppState;
use crate::error::Result;
use crate::models::{AuthResponse, CreateUserRequest, LoginRequest};
use axum::{
    extract::State,
    routing::{get, post},
    Json, Router,
};
use std::sync::Arc;
use validator::Validate;

pub fn auth_routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/register", post(register))
        .route("/login", post(login))
        .route("/me", get(me))
}

async fn register(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateUserRequest>,
) -> Result<Json<AuthResponse>> {
    req.validate()
        .map_err(|e| crate::error::AppError::Validation(e.to_string()))?;

    let response = state.auth_service.register(req).await?;
    Ok(Json(response))
}

async fn login(
    State(state): State<Arc<AppState>>,
    Json(req): Json<LoginRequest>,
) -> Result<Json<AuthResponse>> {
    req.validate()
        .map_err(|e| crate::error::AppError::Validation(e.to_string()))?;

    let response = state.auth_service.login(req).await?;
    Ok(Json(response))
}

async fn me(
    State(state): State<Arc<AppState>>,
    crate::api::middleware::RequireAuth(claims): crate::api::middleware::RequireAuth,
) -> Result<Json<crate::models::UserInfo>> {
    let user = state.auth_service.get_user_by_id(claims.sub).await?;
    Ok(Json(user.into()))
}
