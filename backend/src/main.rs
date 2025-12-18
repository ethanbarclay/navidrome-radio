mod api;
mod config;
mod error;
mod frontend;
mod models;
mod services;

use crate::api::stations::AppState;
use crate::config::Config;
use crate::services::{
    audio_encoder::{AudioEncoder, AudioEncoderConfig},
    hybrid_curator::{HybridCurator, HybridCurationConfig},
    library_indexer::{LibraryIndexer, TrackAnalyzer},
    AiCurator, AuthService, CurationEngine, NavidromeClient, StationManager,
};
use std::path::PathBuf;
use axum::{
    http::{header, Method},
    routing::get,
    Router,
};
use sqlx::postgres::PgPoolOptions;
use std::sync::Arc;
use tower_http::{
    compression::CompressionLayer,
    cors::{Any, CorsLayer},
};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,navidrome_radio=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Load configuration
    let config = Config::from_env()?;
    tracing::info!("Configuration loaded");

    // Connect to database
    let db = PgPoolOptions::new()
        .max_connections(50)
        .connect(&config.database_url)
        .await?;
    tracing::info!("Connected to database");

    // Run migrations
    sqlx::migrate!("./migrations")
        .run(&db)
        .await?;
    tracing::info!("Database migrations completed");

    // Connect to Redis
    let redis_client = redis::Client::open(config.redis_url.as_str())?;
    let redis = redis::aio::ConnectionManager::new(redis_client).await?;
    tracing::info!("Connected to Redis");

    // Initialize services
    let navidrome_client = Arc::new(NavidromeClient::new(
        config.navidrome_url.clone(),
        config.navidrome_user.clone(),
        config.navidrome_password.clone(),
    ));

    let auth_service = Arc::new(AuthService::new(db.clone(), &config));
    let curation_engine = Arc::new(CurationEngine::new(navidrome_client.clone(), &config));
    let station_manager = Arc::new(StationManager::new(
        db.clone(),
        redis.clone(),
        curation_engine.clone(),
        navidrome_client.clone(),
    ));

    // Initialize library indexing services
    let track_analyzer = config.anthropic_api_key.as_ref().map(|api_key| {
        Arc::new(TrackAnalyzer::new(api_key.clone()))
    });

    let library_indexer = Arc::new(LibraryIndexer::new(
        db.clone(),
        navidrome_client.clone(),
        track_analyzer,
    ));

    let ai_curator = config.anthropic_api_key.as_ref().map(|api_key| {
        Arc::new(AiCurator::new(api_key.clone(), db.clone()))
    });

    if ai_curator.is_some() {
        tracing::info!("AI-powered library indexing enabled");
    } else {
        tracing::warn!("AI features disabled - ANTHROPIC_API_KEY not set");
    }

    // Initialize audio encoder (optional - requires ONNX model)
    // Will auto-download from GitHub releases if not found locally
    let audio_encoder = initialize_audio_encoder(&config, &db).await;

    // Initialize hybrid curator (optional - requires both API key and audio encoder)
    let hybrid_curator = match (&config.anthropic_api_key, &audio_encoder) {
        (Some(api_key), Some(encoder)) => {
            let curator = HybridCurator::new(
                api_key.clone(),
                Some(encoder.clone()),
                db.clone(),
                HybridCurationConfig::default(),
                config.navidrome_library_path.clone().map(std::path::PathBuf::from),
            );
            tracing::info!("Hybrid curator initialized (ML + LLM curation enabled)");
            Some(Arc::new(curator))
        }
        (Some(_), None) => {
            tracing::info!("Hybrid curator disabled - audio encoder not available");
            None
        }
        (None, _) => {
            tracing::info!("Hybrid curator disabled - no API key");
            None
        }
    };

    let app_state = Arc::new(AppState {
        db: db.clone(),
        auth_service: auth_service.clone(),
        station_manager: station_manager.clone(),
        curation_engine: curation_engine.clone(),
        library_indexer: library_indexer.clone(),
        ai_curator: ai_curator.clone(),
        audio_encoder,
        hybrid_curator,
        navidrome_library_path: config.navidrome_library_path.clone(),
        embedding_control: Arc::new(tokio::sync::RwLock::new(
            crate::api::stations::EmbeddingControlState::default(),
        )),
    });

    // Load active stations on startup
    if let Err(e) = station_manager.load_active_stations().await {
        tracing::error!("Failed to load active stations: {:?}", e);
    }

    // Build router
    let app = Router::new()
        // API routes
        .nest(
            "/api/v1",
            Router::new()
                .nest("/auth", api::auth_routes())
                .merge(api::station_routes())
                .merge(api::library_routes())
                .nest("/navidrome", api::streaming_routes().with_state(navidrome_client.clone()))
                .with_state(app_state.clone()),
        )
        // Frontend SPA - catch-all route (must be last)
        .fallback(get(frontend::serve_frontend))
        // Middleware
        .layer(CompressionLayer::new())
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods([Method::GET, Method::POST, Method::PATCH, Method::DELETE])
                .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE]),
        );

    // Start server
    let addr = format!("{}:{}", config.server_host, config.server_port);
    tracing::info!("Starting server on {}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

/// GitHub releases URL for the audio encoder model
const MODEL_RELEASE_URL: &str = "https://github.com/ethanbarclay/navidrome-radio/releases/latest/download/audio_encoder.onnx";

/// Default model locations to check
const MODEL_PATHS: &[&str] = &[
    "/app/models/audio_encoder.onnx",      // Docker
    "models/audio_encoder.onnx",           // Local dev (from backend dir)
    "backend/models/audio_encoder.onnx",   // Project root
];

/// Initialize audio encoder, downloading the model if necessary
async fn initialize_audio_encoder(
    config: &Config,
    db: &sqlx::PgPool,
) -> Option<Arc<AudioEncoder>> {
    // Check env var first
    if let Some(ref env_path) = config.audio_encoder_model_path {
        let path = PathBuf::from(env_path);
        if path.exists() {
            return create_audio_encoder(path, db);
        }
        tracing::warn!("AUDIO_ENCODER_MODEL_PATH set but file not found: {:?}", path);
    }

    // Check default locations
    for path_str in MODEL_PATHS {
        let path = PathBuf::from(path_str);
        if path.exists() {
            tracing::info!("Found audio encoder model at: {:?}", path);
            return create_audio_encoder(path, db);
        }
    }

    // Model not found locally - try to download
    tracing::info!("Audio encoder model not found locally, attempting download...");

    // Use first writable location (prefer /app/models in Docker, else local models/)
    let download_path = if PathBuf::from("/app").exists() {
        PathBuf::from("/app/models/audio_encoder.onnx")
    } else {
        PathBuf::from("models/audio_encoder.onnx")
    };

    match download_model(&download_path).await {
        Ok(()) => {
            tracing::info!("Successfully downloaded audio encoder model to {:?}", download_path);
            create_audio_encoder(download_path, db)
        }
        Err(e) => {
            tracing::warn!("Failed to download audio encoder model: {}. ML features will be disabled.", e);
            None
        }
    }
}

/// Download the ONNX model from GitHub releases
async fn download_model(dest: &PathBuf) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use tokio::io::AsyncWriteExt;

    // Create parent directory if needed
    if let Some(parent) = dest.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    tracing::info!("Downloading audio encoder model from GitHub releases...");
    tracing::info!("URL: {}", MODEL_RELEASE_URL);

    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()?;

    let response = client.get(MODEL_RELEASE_URL).send().await?;

    if !response.status().is_success() {
        return Err(format!("HTTP {}: {}", response.status(), MODEL_RELEASE_URL).into());
    }

    let total_size = response.content_length().unwrap_or(0);
    if total_size > 0 {
        tracing::info!("Model size: {:.1} MB", total_size as f64 / 1_000_000.0);
    }

    let bytes = response.bytes().await?;

    let mut file = tokio::fs::File::create(dest).await?;
    file.write_all(&bytes).await?;
    file.flush().await?;

    tracing::info!("Download complete: {:?} ({:.1} MB)", dest, bytes.len() as f64 / 1_000_000.0);
    Ok(())
}

/// Create an AudioEncoder instance from a model path
fn create_audio_encoder(path: PathBuf, db: &sqlx::PgPool) -> Option<Arc<AudioEncoder>> {
    let encoder_config = AudioEncoderConfig {
        model_path: path.clone(),
        ..Default::default()
    };

    match AudioEncoder::new(encoder_config, db.clone()) {
        Ok(encoder) => {
            tracing::info!("Audio encoder initialized from: {:?}", path);
            Some(Arc::new(encoder))
        }
        Err(e) => {
            tracing::warn!("Failed to initialize audio encoder: {}", e);
            None
        }
    }
}
