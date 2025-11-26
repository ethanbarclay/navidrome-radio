use crate::services::NavidromeClient;
use axum::{
    body::Body,
    extract::{Path, State},
    http::{header, StatusCode},
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

    let status_code = response.status().as_u16();

    // Convert headers from reqwest to axum
    let mut builder = Response::builder().status(status_code);

    // Copy important headers using string lookups
    if let Some(content_type) = response.headers().get("content-type") {
        if let Ok(value) = content_type.to_str() {
            builder = builder.header(header::CONTENT_TYPE, value);
        }
    }
    if let Some(content_length) = response.headers().get("content-length") {
        if let Ok(value) = content_length.to_str() {
            builder = builder.header(header::CONTENT_LENGTH, value);
        }
    }

    // Enable range requests for audio seeking
    builder = builder.header(header::ACCEPT_RANGES, "bytes");

    let body = Body::from_stream(response.bytes_stream());

    Ok(builder.body(body).unwrap())
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

    let status_code = response.status().as_u16();

    // Convert headers from reqwest to axum
    let mut builder = Response::builder().status(status_code);

    // Copy content type using string lookup
    if let Some(content_type) = response.headers().get("content-type") {
        if let Ok(value) = content_type.to_str() {
            builder = builder.header(header::CONTENT_TYPE, value);
        }
    }

    // Enable caching for covers
    builder = builder.header(header::CACHE_CONTROL, "public, max-age=3600");

    let bytes = response.bytes().await.map_err(|_| StatusCode::BAD_GATEWAY)?;

    Ok(builder.body(Body::from(bytes)).unwrap())
}
