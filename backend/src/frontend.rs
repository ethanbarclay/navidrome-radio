use axum::{
    body::Body,
    extract::Request,
    http::{header, StatusCode, Uri},
    response::{IntoResponse, Response},
};
use rust_embed::RustEmbed;

// Embed the frontend build directory (SvelteKit static build)
#[derive(RustEmbed)]
#[folder = "../frontend/build"]
pub struct Assets;

pub async fn serve_frontend(uri: Uri) -> impl IntoResponse {
    let path = uri.path().trim_start_matches('/');

    // Try to serve the requested file
    if let Some(content) = Assets::get(path) {
        return serve_asset(path, content.data.into_owned());
    }

    // If not found, check if it's a directory index
    let index_path = format!("{}/index.html", path);
    if let Some(content) = Assets::get(&index_path) {
        return serve_asset(&index_path, content.data.into_owned());
    }

    // For SPA routing, fall back to index.html for non-API routes
    if !path.starts_with("api/") {
        if let Some(content) = Assets::get("index.html") {
            return serve_asset("index.html", content.data.into_owned());
        }
    }

    // File not found
    not_found()
}

fn serve_asset(path: &str, data: Vec<u8>) -> Response {
    let mime = mime_guess::from_path(path).first_or_octet_stream();

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, mime.as_ref())
        .header(header::CACHE_CONTROL, cache_control_value(path))
        .body(Body::from(data))
        .unwrap()
}

fn cache_control_value(path: &str) -> &'static str {
    // Immutable assets (hashed filenames in _app/immutable/)
    if path.starts_with("_app/immutable/") {
        "public, max-age=31536000, immutable"
    }
    // Mutable assets (index.html, etc.)
    else {
        "public, max-age=0, must-revalidate"
    }
}

fn not_found() -> Response {
    Response::builder()
        .status(StatusCode::NOT_FOUND)
        .body(Body::from("404 Not Found"))
        .unwrap()
}
