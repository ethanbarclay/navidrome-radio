mod api;
mod config;
mod error;
mod frontend;
mod models;
mod services;

use crate::api::stations::AppState;
use crate::config::Config;
use crate::services::{AuthService, CurationEngine, NavidromeClient, StationManager};
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

    let app_state = Arc::new(AppState {
        db: db.clone(),
        auth_service: auth_service.clone(),
        station_manager: station_manager.clone(),
        curation_engine: curation_engine.clone(),
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
