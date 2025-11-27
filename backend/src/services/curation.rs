use crate::config::Config;
use crate::error::{AppError, Result};
use crate::models::{SelectionMode, Station, Track};
use crate::services::navidrome::NavidromeClient;
use anyhow::anyhow;
use rand::{seq::SliceRandom, Rng};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::Arc;

pub struct CurationEngine {
    navidrome_client: Arc<NavidromeClient>,
    anthropic_api_key: Option<String>,
    http_client: Client,
}

#[derive(Debug, Serialize)]
struct ClaudeRequest {
    model: String,
    max_tokens: u32,
    messages: Vec<ClaudeMessage>,
}

#[derive(Debug, Serialize)]
struct ClaudeMessage {
    role: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct ClaudeResponse {
    content: Vec<ClaudeContent>,
}

#[derive(Debug, Deserialize)]
struct ClaudeContent {
    text: String,
}

impl CurationEngine {
    pub fn new(navidrome_client: Arc<NavidromeClient>, config: &Config) -> Self {
        Self {
            navidrome_client,
            anthropic_api_key: config.anthropic_api_key.clone(),
            http_client: Client::new(),
        }
    }

    pub fn has_ai_capabilities(&self) -> bool {
        self.anthropic_api_key.is_some()
    }

    pub async fn analyze_description_and_find_tracks(&self, description: &str) -> Result<(Vec<String>, Vec<Track>)> {
        let api_key = self.anthropic_api_key.as_ref().ok_or_else(|| {
            AppError::Internal(anyhow::anyhow!("Anthropic API key not configured"))
        })?;

        // First, get AI to generate search queries
        let prompt = format!(
            "You are a music library curator. Given this radio station description: \"{}\"\n\n\
            Generate 3-5 search queries that would help find appropriate tracks in a music library. \
            These queries should be simple keywords or genre names that would match song metadata (artist, album, genre, title).\n\
            Return ONLY a comma-separated list of search queries, nothing else.\n\
            Examples:\n\
            - For 'Chill vibes for late night coding': \"ambient, electronic, chillout, downtempo, lo-fi\"\n\
            - For 'Energetic workout music': \"electronic, dance, edm, workout, upbeat\"\n\
            - For 'Classic rock from the 70s': \"rock, classic rock, 70s, guitar\"\n\n\
            Your response:",
            description
        );

        let request = ClaudeRequest {
            model: "claude-3-5-haiku-20241022".to_string(),
            max_tokens: 300,
            messages: vec![ClaudeMessage {
                role: "user".to_string(),
                content: prompt,
            }],
        };

        let response = self
            .http_client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&request)
            .send()
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!("Claude API request failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(AppError::Internal(anyhow::anyhow!(
                "Claude API error {}: {}",
                status, error_text
            )));
        }

        let claude_response: ClaudeResponse = response
            .json()
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to parse Claude response: {}", e)))?;

        let text = claude_response
            .content
            .first()
            .map(|c| c.text.as_str())
            .unwrap_or("");

        // Parse comma-separated search queries
        let search_queries: Vec<String> = text
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        if search_queries.is_empty() {
            return Err(AppError::Internal(
                anyhow::anyhow!("Failed to generate search queries from description"),
            ));
        }

        tracing::info!("AI generated search queries: {:?}", search_queries);

        // Multi-step search strategy with fallbacks
        let mut all_tracks = Vec::new();
        let mut seen_ids = HashSet::new();

        // Step 1: Try AI-generated queries
        tracing::info!("Step 1: Searching with AI-generated queries");
        for query in &search_queries {
            tracing::debug!("Searching Navidrome for AI query: {}", query);
            match self.navidrome_client.search_tracks(query, 30).await {
                Ok(tracks) => {
                    tracing::debug!("Found {} tracks for query: {}", tracks.len(), query);
                    for track in tracks {
                        if seen_ids.insert(track.id.clone()) {
                            all_tracks.push(track);
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to search for query '{}': {:?}", query, e);
                }
            }
        }

        // Step 2: If no results, try broader single-word terms from queries
        if all_tracks.is_empty() {
            tracing::info!("Step 2: No results from AI queries, trying individual words");
            let mut words = HashSet::new();
            for query in &search_queries {
                for word in query.split_whitespace() {
                    if word.len() >= 3 {  // Only meaningful words
                        words.insert(word.to_lowercase());
                    }
                }
            }

            for word in words.iter().take(10) {  // Limit to 10 words
                match self.navidrome_client.search_tracks(word, 20).await {
                    Ok(tracks) => {
                        tracing::debug!("Found {} tracks for word: {}", tracks.len(), word);
                        for track in tracks {
                            if seen_ids.insert(track.id.clone()) {
                                all_tracks.push(track);
                            }
                        }
                    }
                    Err(e) => {
                        tracing::debug!("No tracks for word '{}': {:?}", word, e);
                    }
                }
            }
        }

        // Step 3: If still no results, try common genre terms
        if all_tracks.is_empty() {
            tracing::info!("Step 3: No results from words, trying common genres");
            let common_genres = vec!["rock", "pop", "electronic", "jazz", "classical", "metal", "folk", "indie"];
            for genre in &common_genres {
                match self.navidrome_client.search_tracks(genre, 20).await {
                    Ok(tracks) => {
                        if !tracks.is_empty() {
                            tracing::debug!("Found {} tracks for genre: {}", tracks.len(), genre);
                            for track in tracks {
                                if seen_ids.insert(track.id.clone()) {
                                    all_tracks.push(track);
                                }
                            }
                            break;  // Found some tracks, stop searching common genres
                        }
                    }
                    Err(e) => {
                        tracing::debug!("No tracks for genre '{}': {:?}", genre, e);
                    }
                }
            }
        }

        // Step 4: Last resort - get ANY tracks from library using common letters
        if all_tracks.is_empty() {
            tracing::info!("Step 4: Last resort - searching for any tracks with common search terms");
            let fallback_queries = vec!["a", "the", "e", "s", "t"];
            for query in &fallback_queries {
                match self.navidrome_client.search_tracks(query, 50).await {
                    Ok(tracks) => {
                        if !tracks.is_empty() {
                            tracing::info!("Fallback found {} tracks with query: {}", tracks.len(), query);
                            for track in tracks {
                                if seen_ids.insert(track.id.clone()) {
                                    all_tracks.push(track);
                                }
                            }
                            break;  // Got some tracks, done
                        }
                    }
                    Err(e) => {
                        tracing::debug!("Fallback query '{}' failed: {:?}", query, e);
                    }
                }
            }
        }

        if all_tracks.is_empty() {
            return Err(AppError::NotFound(
                "Library appears to be empty or Navidrome is not accessible".to_string(),
            ));
        }

        tracing::info!("Successfully found {} total matching tracks from library", all_tracks.len());

        // Return both the queries (as genres) and the matching tracks
        Ok((search_queries, all_tracks))
    }

    pub async fn select_next_track(
        &self,
        station: &Station,
        recent_track_ids: &[String],
    ) -> Result<Track> {
        // If station has curated track_ids, use those instead of genre-based selection
        if !station.track_ids.is_empty() {
            tracing::info!("Station '{}' has {} curated tracks, selecting from those", station.name, station.track_ids.len());
            return self.select_from_curated(station, recent_track_ids).await;
        }
        tracing::debug!("Station '{}' has no curated tracks, using genre-based selection", station.name);

        match station.config.track_selection_mode {
            SelectionMode::Random | SelectionMode::Hybrid => {
                self.select_random(station, recent_track_ids).await
            }
            SelectionMode::AIContextual | SelectionMode::AIEmbeddings => {
                // Fall back to random if AI is not configured
                if self.anthropic_api_key.is_some() {
                    // TODO: Implement AI selection
                    tracing::warn!("AI selection not yet implemented, falling back to random");
                    self.select_random(station, recent_track_ids).await
                } else {
                    self.select_random(station, recent_track_ids).await
                }
            }
        }
    }

    /// Select a track from the station's curated track_ids list
    async fn select_from_curated(
        &self,
        station: &Station,
        recent_track_ids: &[String],
    ) -> Result<Track> {
        let recent_set: HashSet<_> = recent_track_ids.iter().collect();

        // Filter out recently played tracks
        let available_ids: Vec<&String> = station
            .track_ids
            .iter()
            .filter(|id| !recent_set.contains(id))
            .collect();

        // If all tracks have been played recently, reset and use all tracks
        let mut candidates: Vec<&String> = if available_ids.is_empty() {
            tracing::info!("All curated tracks played recently, resetting pool");
            station.track_ids.iter().collect()
        } else {
            available_ids
        };

        // Duration filters
        let min_dur = station.config.min_track_duration as i32;
        let max_dur = station.config.max_track_duration as i32;

        // Try to find a valid track, removing invalid ones from candidates
        let mut tried_ids: HashSet<&String> = HashSet::new();

        while !candidates.is_empty() {
            // Pick a random track ID from the remaining candidates
            let idx = rand::thread_rng().gen_range(0..candidates.len());
            let track_id = candidates[idx];

            // Skip if we've already tried this one
            if tried_ids.contains(track_id) {
                candidates.remove(idx);
                continue;
            }
            tried_ids.insert(track_id);

            // Fetch the track details from Navidrome
            match self.navidrome_client.get_track(track_id).await {
                Ok(track) => {
                    // Check duration requirements
                    if track.duration >= min_dur && track.duration <= max_dur {
                        tracing::info!("Selected curated track: {} - {}", track.artist, track.title);
                        return Ok(track);
                    }
                    // Track doesn't meet duration requirements, remove from candidates
                    candidates.remove(idx);
                }
                Err(e) => {
                    tracing::warn!("Failed to fetch track {}: {:?}", track_id, e);
                    candidates.remove(idx);
                }
            }
        }

        Err(AppError::NotFound("No suitable curated tracks found".to_string()))
    }

    async fn select_random(
        &self,
        station: &Station,
        recent_track_ids: &[String],
    ) -> Result<Track> {
        let mut all_candidates = Vec::new();

        // Handle wildcard or multiple genres
        if station.genres.iter().any(|g| g == "*") {
            // Wildcard: search broadly
            tracing::debug!("Searching Navidrome with wildcard query: a");
            all_candidates = self.navidrome_client.search_tracks("a", 50).await?;
        } else if station.genres.len() > 1 {
            // Multiple genres: search each separately and combine
            let mut seen_ids = HashSet::new();
            for genre in &station.genres {
                tracing::debug!("Searching Navidrome for genre: {}", genre);
                let tracks = self.navidrome_client.search_tracks(genre, 50).await?;
                tracing::debug!("Found {} tracks for genre: {}", tracks.len(), genre);

                // Add tracks we haven't seen yet
                for track in tracks {
                    if seen_ids.insert(track.id.clone()) {
                        all_candidates.push(track);
                    }
                }
            }
        } else {
            // Single genre: search directly
            let query = station.genres.first().map(|s| s.as_str()).unwrap_or("a");
            tracing::debug!("Searching Navidrome with single genre: {}", query);
            all_candidates = self.navidrome_client.search_tracks(query, 50).await?;
        }

        tracing::debug!("Found {} total candidates from Navidrome", all_candidates.len());

        // Filter out recently played tracks
        let recent_set: HashSet<_> = recent_track_ids.iter().collect();
        all_candidates.retain(|t| !recent_set.contains(&t.id));

        // Filter by duration
        let min_dur = station.config.min_track_duration as i32;
        let max_dur = station.config.max_track_duration as i32;
        all_candidates.retain(|t| t.duration >= min_dur && t.duration <= max_dur);

        // Select random track
        all_candidates
            .choose(&mut rand::thread_rng())
            .cloned()
            .ok_or_else(|| {
                crate::error::AppError::NotFound("No suitable tracks found".to_string())
            })
    }
}
