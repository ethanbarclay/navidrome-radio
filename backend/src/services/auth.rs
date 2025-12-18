use crate::config::Config;
use crate::error::{AppError, Result};
use crate::models::{AuthResponse, CreateUserRequest, LoginRequest, User, UserInfo, UserRole};
use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use chrono::{Duration, Utc};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: Uuid,
    pub role: UserRole,
    pub exp: i64,
}

pub struct AuthService {
    db: PgPool,
    jwt_secret: String,
}

impl AuthService {
    pub fn new(db: PgPool, config: &Config) -> Self {
        Self {
            db,
            jwt_secret: config.jwt_secret.clone(),
        }
    }

    pub async fn register(&self, req: CreateUserRequest) -> Result<AuthResponse> {
        // Hash password
        let password_hash = self.hash_password(&req.password)?;

        // Check if this is the first user (make them admin)
        let user_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
            .fetch_one(&self.db)
            .await?;

        let role = if user_count == 0 {
            UserRole::Admin // First user is always admin
        } else {
            req.role.unwrap_or(UserRole::Listener)
        };

        // Create user
        let user = sqlx::query_as::<_, User>(
            r#"
            INSERT INTO users (username, email, password_hash, role)
            VALUES ($1, $2, $3, $4)
            RETURNING *
            "#,
        )
        .bind(&req.username)
        .bind(&req.email)
        .bind(&password_hash)
        .bind(role)
        .fetch_one(&self.db)
        .await
        .map_err(|e| {
            if e.to_string().contains("unique") {
                AppError::Validation("Username or email already exists".to_string())
            } else {
                AppError::Database(e)
            }
        })?;

        // Generate token
        let token = self.generate_token(&user)?;

        Ok(AuthResponse {
            token,
            user: user.into(),
        })
    }

    pub async fn login(&self, req: LoginRequest) -> Result<AuthResponse> {
        // Fetch user
        let user = sqlx::query_as::<_, User>(
            "SELECT * FROM users WHERE username = $1",
        )
        .bind(&req.username)
        .fetch_optional(&self.db)
        .await?
        .ok_or(AppError::InvalidCredentials)?;

        // Verify password
        self.verify_password(&req.password, &user.password_hash)?;

        // Update last login
        sqlx::query("UPDATE users SET last_login = NOW() WHERE id = $1")
            .bind(user.id)
            .execute(&self.db)
            .await?;

        // Generate token
        let token = self.generate_token(&user)?;

        Ok(AuthResponse {
            token,
            user: user.into(),
        })
    }

    pub async fn verify_token(&self, token: &str) -> Result<Claims> {
        let token_data = decode::<Claims>(
            token,
            &DecodingKey::from_secret(self.jwt_secret.as_bytes()),
            &Validation::default(),
        )
        .map_err(|_| AppError::Unauthorized)?;

        Ok(token_data.claims)
    }

    /// Validate that a token belongs to an admin user
    pub async fn validate_admin_token(&self, token: &str) -> Result<Claims> {
        let claims = self.verify_token(token).await?;
        if claims.role != UserRole::Admin {
            return Err(AppError::Forbidden);
        }
        Ok(claims)
    }

    pub async fn get_user_by_id(&self, user_id: Uuid) -> Result<User> {
        sqlx::query_as::<_, User>("SELECT * FROM users WHERE id = $1")
            .bind(user_id)
            .fetch_optional(&self.db)
            .await?
            .ok_or_else(|| AppError::NotFound("User not found".to_string()))
    }

    fn hash_password(&self, password: &str) -> Result<String> {
        let salt = SaltString::generate(&mut OsRng);
        let argon2 = Argon2::default();

        argon2
            .hash_password(password.as_bytes(), &salt)
            .map(|hash| hash.to_string())
            .map_err(|e| AppError::Internal(anyhow::anyhow!("Password hashing failed: {}", e)))
    }

    fn verify_password(&self, password: &str, password_hash: &str) -> Result<()> {
        let parsed_hash =
            PasswordHash::new(password_hash)
                .map_err(|e| AppError::Internal(anyhow::anyhow!("Invalid password hash: {}", e)))?;

        Argon2::default()
            .verify_password(password.as_bytes(), &parsed_hash)
            .map_err(|_| AppError::InvalidCredentials)
    }

    fn generate_token(&self, user: &User) -> Result<String> {
        let claims = Claims {
            sub: user.id,
            role: user.role.clone(),
            exp: (Utc::now() + Duration::days(7)).timestamp(),
        };

        encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(self.jwt_secret.as_bytes()),
        )
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Token generation failed: {}", e)))
    }
}
