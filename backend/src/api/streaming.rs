use crate::services::NavidromeClient;
use axum::{
    body::Body,
    extract::{Path, State},
    http::{header, HeaderMap, StatusCode},
    response::Response,
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

    // Proxy the stream through our backend
    let client = reqwest::Client::new();
    let response = client
        .get(&stream_url)
        .send()
        .await
        .map_err(|_| StatusCode::BAD_GATEWAY)?;

    let status = response.status();
    let mut headers = HeaderMap::new();

    // Copy important headers
    if let Some(content_type) = response.headers().get(header::CONTENT_TYPE) {
        headers.insert(header::CONTENT_TYPE, content_type.clone());
    }
    if let Some(content_length) = response.headers().get(header::CONTENT_LENGTH) {
        headers.insert(header::CONTENT_LENGTH, content_length.clone());
    }

    // Enable range requests for audio seeking
    headers.insert(header::ACCEPT_RANGES, "bytes".parse().unwrap());

    let body = Body::from_stream(response.bytes_stream());

    Ok(Response::builder()
        .status(status)
        .body(body)
        .unwrap())
}

async fn get_cover(
    State(navidrome): State<Arc<NavidromeClient>>,
    Path(track_id): Path<String>,
) -> Result<Response, StatusCode> {
    let cover_url = navidrome.get_cover_url(&track_id).await;

    // Proxy the cover through our backend
    let client = reqwest::Client::new();
    let response = client
        .get(&cover_url)
        .send()
        .await
        .map_err(|_| StatusCode::BAD_GATEWAY)?;

    let status = response.status();
    let mut headers = HeaderMap::new();

    // Copy content type
    if let Some(content_type) = response.headers().get(header::CONTENT_TYPE) {
        headers.insert(header::CONTENT_TYPE, content_type.clone());
    }

    // Enable caching for covers
    headers.insert(header::CACHE_CONTROL, "public, max-age=3600".parse().unwrap());

    let bytes = response.bytes().await.map_err(|_| StatusCode::BAD_GATEWAY)?;

    Ok(Response::builder()
        .status(status)
        .body(Body::from(bytes))
        .unwrap())
}
