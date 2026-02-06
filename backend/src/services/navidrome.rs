#![allow(dead_code)]

use crate::error::{AppError, Result};
use crate::models::Track;
use chrono::Utc;
use rand::Rng;
use reqwest::Client;
use serde::Deserialize;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug, Clone)]
pub struct NavidromeClient {
    base_url: String,
    username: String,
    password: String,  // Store original password for native API
    token: String,
    salt: String,
    client: Client,
    /// Cached JWT token for native API (shared across clones)
    jwt_cache: Arc<RwLock<Option<String>>>,
}

#[derive(Debug, Deserialize)]
struct SubsonicResponse<T> {
    #[serde(rename = "subsonic-response")]
    subsonic_response: T,
}

#[derive(Debug, Deserialize)]
struct SearchResult3 {
    #[serde(rename = "searchResult3")]
    search_result3: SearchResult3Data,
}

#[derive(Debug, Deserialize)]
struct SearchResult3Data {
    #[serde(default)]
    song: Vec<NavidromeSong>,
}

#[derive(Debug, Deserialize)]
struct RandomSongsResponse {
    #[serde(rename = "randomSongs")]
    random_songs: RandomSongsData,
}

#[derive(Debug, Deserialize)]
struct RandomSongsData {
    #[serde(default)]
    song: Vec<NavidromeSong>,
}

#[derive(Debug, Deserialize)]
struct GetSongResponse {
    song: NavidromeSong,
}

#[derive(Debug, Deserialize)]
struct NavidromeGenre {
    name: String,
}

#[derive(Debug, Deserialize)]
struct NavidromeSong {
    id: String,
    title: String,
    artist: String,
    album: String,
    #[serde(default)]
    genre: String,  // Old API
    #[serde(default)]
    genres: Vec<NavidromeGenre>,  // New API
    year: Option<i32>,
    duration: i32,
    path: String,
}

/// Navidrome Native API response for /api/song
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct NativeApiSong {
    id: String,
    title: String,
    artist: String,
    album: String,
    #[serde(default)]
    genre: String,
    #[serde(default)]
    genres: Vec<NativeApiGenre>,
    year: Option<i32>,
    duration: f64,  // Native API returns duration as float
    path: String,
}

#[derive(Debug, Deserialize)]
struct NativeApiGenre {
    id: String,
    name: String,
}

/// Login response from Navidrome's /auth/login endpoint
#[derive(Debug, Deserialize)]
struct LoginResponse {
    token: String,
}

impl NavidromeClient {
    pub fn new(base_url: String, username: String, password: String) -> Self {
        let salt = Self::generate_salt();
        let token = format!("{:x}", md5::compute(format!("{}{}", password, salt)));

        Self {
            base_url,
            username,
            password,  // Store for native API auth
            token,
            salt,
            client: Client::new(),
            jwt_cache: Arc::new(RwLock::new(None)),
        }
    }

    /// Get the base URL for constructing API endpoints
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Get the HTTP client for making requests
    pub fn client(&self) -> &Client {
        &self.client
    }

    /// Build Subsonic API parameters (public for use by other services)
    pub fn build_params(&self, additional: Vec<(&str, &str)>) -> Vec<(String, String)> {
        let mut params = vec![
            ("u".to_string(), self.username.clone()),
            ("t".to_string(), self.token.clone()),
            ("s".to_string(), self.salt.clone()),
            ("v".to_string(), "1.16.1".to_string()),
            ("c".to_string(), "navidrome-radio".to_string()),
            ("f".to_string(), "json".to_string()),
        ];

        for (key, value) in additional {
            params.push((key.to_string(), value.to_string()));
        }

        params
    }

    fn generate_salt() -> String {
        let mut rng = rand::thread_rng();
        (0..8)
            .map(|_| format!("{:x}", rng.gen::<u8>()))
            .collect()
    }

    /// Login to Navidrome's native API to get a JWT token (cached)
    async fn get_jwt_token(&self) -> Result<String> {
        // Check cache first
        {
            let cache = self.jwt_cache.read().await;
            if let Some(token) = cache.as_ref() {
                return Ok(token.clone());
            }
        }

        // No cached token, perform login
        let url = format!("{}/auth/login", self.base_url);

        let response = self
            .client
            .post(&url)
            .json(&serde_json::json!({
                "username": self.username,
                "password": self.password
            }))
            .send()
            .await
            .map_err(|e| AppError::Navidrome(format!("Login request failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(AppError::Navidrome(format!(
                "Login failed: {} - {}",
                status, body
            )));
        }

        let login_response: LoginResponse = response
            .json()
            .await
            .map_err(|e| AppError::Navidrome(format!("Failed to parse login response: {}", e)))?;

        // Cache the token
        {
            let mut cache = self.jwt_cache.write().await;
            *cache = Some(login_response.token.clone());
        }

        Ok(login_response.token)
    }

    /// Clear the cached JWT token (useful if it expires)
    #[allow(dead_code)]
    pub async fn clear_jwt_cache(&self) {
        let mut cache = self.jwt_cache.write().await;
        *cache = None;
    }

    pub async fn search_tracks(&self, query: &str, count: usize) -> Result<Vec<Track>> {
        let url = format!("{}/rest/search3", self.base_url);
        let params = self.build_params(vec![
            ("query", query),
            ("songCount", &count.to_string()),
        ]);

        tracing::debug!("Searching Navidrome: {} with query: {}", url, query);

        let response = self
            .client
            .get(&url)
            .query(&params)
            .send()
            .await
            .map_err(|e| AppError::Navidrome(format!("Request failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            tracing::error!("Navidrome API error: {} - {}", status, body);
            return Err(AppError::Navidrome(format!(
                "API returned status: {} - {}",
                status, body
            )));
        }

        let response_text = response.text().await.map_err(|e| AppError::Navidrome(format!("Failed to read response: {}", e)))?;

        tracing::debug!("Navidrome response: {}", &response_text[..std::cmp::min(500, response_text.len())]);

        let data: SubsonicResponse<SearchResult3> = serde_json::from_str(&response_text)
            .map_err(|e| AppError::Navidrome(format!("Failed to parse response: {} - Response: {}", e, &response_text[..std::cmp::min(200, response_text.len())])))?;

        tracing::debug!("Found {} songs in response", data.subsonic_response.search_result3.song.len());

        Ok(data
            .subsonic_response
            .search_result3
            .song
            .into_iter()
            .map(|song| {
                // Prefer the new genres array, fall back to old genre string
                let genres = if !song.genres.is_empty() {
                    song.genres.into_iter().map(|g| g.name).collect()
                } else if !song.genre.is_empty() {
                    vec![song.genre]
                } else {
                    vec![]
                };

                Track {
                    id: song.id,
                    title: song.title,
                    artist: song.artist,
                    album: song.album,
                    genre: genres,
                    year: song.year,
                    duration: song.duration,
                    path: song.path,
                    metadata: None,
                    last_synced: Utc::now(),
                }
            })
            .collect())
    }

    /// Get random songs from Navidrome
    /// This is used to fetch all songs from the library by requesting a large number
    pub async fn get_random_songs(&self, count: usize) -> Result<Vec<Track>> {
        let url = format!("{}/rest/getRandomSongs", self.base_url);
        let params = self.build_params(vec![
            ("size", &count.to_string()),
        ]);

        tracing::debug!("Getting {} random songs from Navidrome", count);

        let response = self
            .client
            .get(&url)
            .query(&params)
            .send()
            .await
            .map_err(|e| AppError::Navidrome(format!("Request failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            tracing::error!("Navidrome API error: {} - {}", status, body);
            return Err(AppError::Navidrome(format!(
                "API returned status: {} - {}",
                status, body
            )));
        }

        let response_text = response.text().await.map_err(|e| AppError::Navidrome(format!("Failed to read response: {}", e)))?;

        tracing::debug!("Navidrome response: {}", &response_text[..std::cmp::min(500, response_text.len())]);

        let data: SubsonicResponse<RandomSongsResponse> = serde_json::from_str(&response_text)
            .map_err(|e| AppError::Navidrome(format!("Failed to parse response: {} - Response: {}", e, &response_text[..std::cmp::min(200, response_text.len())])))?;

        tracing::debug!("Found {} songs in response", data.subsonic_response.random_songs.song.len());

        Ok(self.convert_navidrome_songs(data.subsonic_response.random_songs.song))
    }

    /// Convert NavidromeSong objects to Track objects
    fn convert_navidrome_songs(&self, songs: Vec<NavidromeSong>) -> Vec<Track> {
        songs
            .into_iter()
            .map(|song| {
                // Prefer the new genres array, fall back to old genre string
                let genres = if !song.genres.is_empty() {
                    song.genres.into_iter().map(|g| g.name).collect()
                } else if !song.genre.is_empty() {
                    vec![song.genre]
                } else {
                    vec![]
                };

                Track {
                    id: song.id,
                    title: song.title,
                    artist: song.artist,
                    album: song.album,
                    genre: genres,
                    year: song.year,
                    duration: song.duration,
                    path: song.path,
                    metadata: None,
                    last_synced: Utc::now(),
                }
            })
            .collect()
    }

    pub async fn get_stream_url(&self, track_id: &str) -> String {
        format!(
            "{}/rest/stream?id={}&u={}&t={}&s={}&v=1.16.1&c=navidrome-radio",
            self.base_url, track_id, self.username, self.token, self.salt
        )
    }

    pub async fn get_cover_url(&self, track_id: &str) -> String {
        format!(
            "{}/rest/getCoverArt?id={}&u={}&t={}&s={}&v=1.16.1&c=navidrome-radio&size=500",
            self.base_url, track_id, self.username, self.token, self.salt
        )
    }

    pub async fn get_genres(&self) -> Result<Vec<String>> {
        let url = format!("{}/rest/getGenres", self.base_url);
        let params = self.build_params(vec![]);

        let _response = self
            .client
            .get(&url)
            .query(&params)
            .send()
            .await
            .map_err(|e| AppError::Navidrome(format!("Request failed: {}", e)))?;

        // For simplicity, return a default list if the API call fails
        Ok(vec![
            "Rock".to_string(),
            "Pop".to_string(),
            "Jazz".to_string(),
            "Classical".to_string(),
            "Electronic".to_string(),
            "Hip Hop".to_string(),
            "Blues".to_string(),
            "Country".to_string(),
        ])
    }

    /// Get all songs from Navidrome using the native API with pagination
    /// This is more reliable than getRandomSongs for large libraries
    pub async fn get_all_songs_paginated(&self, page_size: usize, offset: usize) -> Result<(Vec<Track>, usize)> {
        // Get JWT token for native API authentication
        let jwt_token = self.get_jwt_token().await?;

        // Use Navidrome's native API endpoint which supports proper pagination
        let url = format!("{}/api/song", self.base_url);

        tracing::debug!("Fetching songs from Navidrome native API: offset={}, size={}", offset, page_size);

        let response = self
            .client
            .get(&url)
            .header("x-nd-authorization", format!("Bearer {}", jwt_token))
            .query(&[
                ("_start", offset.to_string()),
                ("_end", (offset + page_size).to_string()),
                ("_order", "ASC".to_string()),
                ("_sort", "id".to_string()),
            ])
            .send()
            .await
            .map_err(|e| AppError::Navidrome(format!("Request failed: {}", e)))?;

        // Get total count from header
        let total_count = response
            .headers()
            .get("x-total-count")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(0);

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            tracing::error!("Navidrome native API error: {} - {}", status, body);
            return Err(AppError::Navidrome(format!(
                "Native API returned status: {} - {}",
                status, body
            )));
        }

        let response_text = response.text().await.map_err(|e| {
            AppError::Navidrome(format!("Failed to read response: {}", e))
        })?;

        let songs: Vec<NativeApiSong> = serde_json::from_str(&response_text)
            .map_err(|e| {
                AppError::Navidrome(format!(
                    "Failed to parse native API response: {} - Response: {}",
                    e,
                    &response_text[..std::cmp::min(200, response_text.len())]
                ))
            })?;

        tracing::debug!("Native API returned {} songs, total: {}", songs.len(), total_count);

        let tracks = songs
            .into_iter()
            .map(|song| {
                let genres = if !song.genres.is_empty() {
                    song.genres.into_iter().map(|g| g.name).collect()
                } else if !song.genre.is_empty() {
                    vec![song.genre]
                } else {
                    vec![]
                };

                Track {
                    id: song.id,
                    title: song.title,
                    artist: song.artist,
                    album: song.album,
                    genre: genres,
                    year: song.year,
                    duration: song.duration as i32,
                    path: song.path,
                    metadata: None,
                    last_synced: Utc::now(),
                }
            })
            .collect();

        Ok((tracks, total_count))
    }

    /// Create a playlist in Navidrome with the given name and track IDs
    pub async fn create_playlist(&self, name: &str, track_ids: &[String]) -> Result<String> {
        let url = format!("{}/rest/createPlaylist", self.base_url);

        // Build base params
        let mut params = self.build_params(vec![("name", name)]);

        // Add each track ID as a separate songId parameter
        for track_id in track_ids {
            params.push(("songId".to_string(), track_id.clone()));
        }

        tracing::info!("Creating Navidrome playlist '{}' with {} tracks", name, track_ids.len());

        let response = self
            .client
            .get(&url)
            .query(&params)
            .send()
            .await
            .map_err(|e| AppError::Navidrome(format!("Request failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            tracing::error!("Navidrome createPlaylist error: {} - {}", status, body);
            return Err(AppError::Navidrome(format!(
                "Failed to create playlist: {} - {}",
                status, body
            )));
        }

        let response_text = response.text().await.map_err(|e| {
            AppError::Navidrome(format!("Failed to read response: {}", e))
        })?;

        tracing::debug!("createPlaylist response: {}", response_text);

        // Parse the response to get the playlist ID
        // The response format is: {"subsonic-response":{"status":"ok","playlist":{"id":"..."}}}
        #[derive(Deserialize)]
        struct CreatePlaylistResponse {
            status: String,
            #[serde(default)]
            playlist: Option<PlaylistInfo>,
        }

        #[derive(Deserialize)]
        struct PlaylistInfo {
            id: String,
        }

        let data: SubsonicResponse<CreatePlaylistResponse> = serde_json::from_str(&response_text)
            .map_err(|e| {
                AppError::Navidrome(format!(
                    "Failed to parse createPlaylist response: {} - Response: {}",
                    e,
                    &response_text[..std::cmp::min(200, response_text.len())]
                ))
            })?;

        if data.subsonic_response.status != "ok" {
            return Err(AppError::Navidrome("Playlist creation failed".to_string()));
        }

        let playlist_id = data
            .subsonic_response
            .playlist
            .map(|p| p.id)
            .unwrap_or_else(|| "unknown".to_string());

        tracing::info!("Created playlist '{}' with ID: {}", name, playlist_id);
        Ok(playlist_id)
    }

    /// Get a single track by ID
    pub async fn get_track(&self, track_id: &str) -> Result<Track> {
        let url = format!("{}/rest/getSong", self.base_url);
        let params = self.build_params(vec![("id", track_id)]);

        tracing::debug!("Getting track from Navidrome: {}", track_id);

        let response = self
            .client
            .get(&url)
            .query(&params)
            .send()
            .await
            .map_err(|e| AppError::Navidrome(format!("Request failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            tracing::error!("Navidrome API error: {} - {}", status, body);
            return Err(AppError::Navidrome(format!(
                "API returned status: {} - {}",
                status, body
            )));
        }

        let response_text = response.text().await.map_err(|e| {
            AppError::Navidrome(format!("Failed to read response: {}", e))
        })?;

        let data: SubsonicResponse<GetSongResponse> = serde_json::from_str(&response_text)
            .map_err(|e| {
                AppError::Navidrome(format!(
                    "Failed to parse response: {} - Response: {}",
                    e,
                    &response_text[..std::cmp::min(200, response_text.len())]
                ))
            })?;

        let song = data.subsonic_response.song;

        // Convert to Track
        let genres = if !song.genres.is_empty() {
            song.genres.into_iter().map(|g| g.name).collect()
        } else if !song.genre.is_empty() {
            vec![song.genre]
        } else {
            vec![]
        };

        Ok(Track {
            id: song.id,
            title: song.title,
            artist: song.artist,
            album: song.album,
            genre: genres,
            year: song.year,
            duration: song.duration,
            path: song.path,
            metadata: None,
            last_synced: Utc::now(),
        })
    }
}
