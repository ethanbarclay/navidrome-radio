use crate::services::NavidromeClient;
use axum::{
    body::Body,
    extract::{Path, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use std::sync::Arc;

pub fn streaming_routes() -> Router<Arc<NavidromeClient>> {
    Router::new()
        .route("/stream/:track_id", get(stream_track))
        .route("/cover/:track_id", get(get_cover))
}

async fn stream_track(
    State(navidrome): State<Arc<NavidromeClient>>,
    Path(track_id): Path<String>,
) -> Result<Response, StatusCode> {
    let stream_url = navidrome.get_stream_url(&track_id).await;

    // Redirect to Navidrome stream URL
    Ok(Response::builder()
        .status(StatusCode::TEMPORARY_REDIRECT)
        .header(header::LOCATION, stream_url)
        .body(Body::empty())
        .unwrap())
}

async fn get_cover(
    State(navidrome): State<Arc<NavidromeClient>>,
    Path(track_id): Path<String>,
) -> Result<Response, StatusCode> {
    let cover_url = navidrome.get_cover_url(&track_id).await;

    // Redirect to Navidrome cover URL
    Ok(Response::builder()
        .status(StatusCode::TEMPORARY_REDIRECT)
        .header(header::LOCATION, cover_url)
        .body(Body::empty())
        .unwrap())
}
