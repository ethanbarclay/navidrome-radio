use std::env;

#[derive(Debug, Clone)]
pub struct Config {
    pub database_url: String,
    pub redis_url: String,
    pub navidrome_url: String,
    pub navidrome_user: String,
    pub navidrome_password: String,
    pub anthropic_api_key: Option<String>,
    pub openai_api_key: Option<String>,
    pub jwt_secret: String,
    pub server_host: String,
    pub server_port: u16,
    /// Path to the Navidrome music library (for audio embedding generation)
    pub navidrome_library_path: Option<String>,
    /// Path to the ONNX audio encoder model
    pub audio_encoder_model_path: Option<String>,
}

impl Config {
    pub fn from_env() -> Result<Self, anyhow::Error> {
        dotenvy::dotenv().ok();

        Ok(Config {
            database_url: env::var("DATABASE_URL")
                .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/navidrome_radio".to_string()),
            redis_url: env::var("REDIS_URL")
                .unwrap_or_else(|_| "redis://localhost:6379".to_string()),
            navidrome_url: env::var("NAVIDROME_URL")
                .expect("NAVIDROME_URL must be set"),
            navidrome_user: env::var("NAVIDROME_USER")
                .expect("NAVIDROME_USER must be set"),
            navidrome_password: env::var("NAVIDROME_PASSWORD")
                .expect("NAVIDROME_PASSWORD must be set"),
            anthropic_api_key: env::var("ANTHROPIC_API_KEY").ok(),
            openai_api_key: env::var("OPENAI_API_KEY").ok(),
            jwt_secret: env::var("JWT_SECRET")
                .unwrap_or_else(|_| "insecure-secret-change-in-production".to_string()),
            server_host: env::var("SERVER_HOST")
                .unwrap_or_else(|_| "0.0.0.0".to_string()),
            server_port: env::var("SERVER_PORT")
                .unwrap_or_else(|_| "8000".to_string())
                .parse()
                .unwrap_or(8000),
            navidrome_library_path: env::var("NAVIDROME_LIBRARY_PATH").ok(),
            audio_encoder_model_path: env::var("AUDIO_ENCODER_MODEL_PATH").ok(),
        })
    }
}
