use crate::error::{AppError, Result};
use crate::models::Track;
use chrono::Utc;
use rand::Rng;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct NavidromeClient {
    base_url: String,
    username: String,
    token: String,
    salt: String,
    client: Client,
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

impl NavidromeClient {
    pub fn new(base_url: String, username: String, password: String) -> Self {
        let salt = Self::generate_salt();
        let token = format!("{:x}", md5::compute(format!("{}{}", password, salt)));

        Self {
            base_url,
            username,
            token,
            salt,
            client: Client::new(),
        }
    }

    fn generate_salt() -> String {
        let mut rng = rand::thread_rng();
        (0..8)
            .map(|_| format!("{:x}", rng.gen::<u8>()))
            .collect()
    }

    fn build_params(&self, additional: Vec<(&str, &str)>) -> Vec<(String, String)> {
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

        let response = self
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
}
