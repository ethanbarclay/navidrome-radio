use crate::api::stations::AppState;
use crate::error::{AppError, Result};
use crate::models::UserRole;
use crate::services::auth::Claims;
use axum::{
    async_trait,
    extract::FromRequestParts,
    http::request::Parts,
};
use std::sync::Arc;

pub struct RequireAuth(pub Claims);

#[async_trait]
impl FromRequestParts<Arc<AppState>> for RequireAuth {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &Arc<AppState>,
    ) -> Result<Self> {
        // Get token from Authorization header
        let token = parts
            .headers
            .get("Authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .ok_or(AppError::Unauthorized)?;

        // Verify token
        let claims = state.auth_service.verify_token(token).await?;

        Ok(RequireAuth(claims))
    }
}

pub struct RequireAdmin(pub Claims);

#[async_trait]
impl FromRequestParts<Arc<AppState>> for RequireAdmin {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &Arc<AppState>,
    ) -> Result<Self> {
        let RequireAuth(claims) = RequireAuth::from_request_parts(parts, state).await?;

        if claims.role != UserRole::Admin {
            return Err(AppError::Forbidden);
        }

        Ok(RequireAdmin(claims))
    }
}
