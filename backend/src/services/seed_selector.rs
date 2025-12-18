//! Seed Selector Service
//!
//! Uses LLM to select high-quality "seed" songs that perfectly match a query.
//! These seeds are then distributed throughout the playlist for the audio encoder
//! to fill in the gaps with sonically similar tracks.
//!
//! Strategy:
//! 1. Ask LLM for ideal songs (artist + title) that would be PERFECT examples
//! 2. Verify each exists in the user's library
//! 3. If not found, ask LLM to pick from a sample of actual library tracks
//! 4. Return verified seeds with their positions in the final playlist

use crate::error::{AppError, Result};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};
use tracing::{debug, info, warn};

/// Simplified track info for seed selection (avoids needing all LibraryTrack fields)
#[derive(Debug, Clone, FromRow)]
struct SeedTrackInfo {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub genres: String, // JSON string - parsed manually to avoid pgvector binary protocol issues
}

impl SeedTrackInfo {
    /// Parse genres from JSON string
    fn genres_vec(&self) -> Vec<String> {
        serde_json::from_str(&self.genres).unwrap_or_default()
    }
}

/// Represents an ideal song suggested by the LLM
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdealSong {
    pub title: String,
    pub artist: String,
    pub reason: String,
}

/// A verified seed with its position in the playlist
#[derive(Debug, Clone)]
pub struct VerifiedSeed {
    pub track_id: String,
    pub title: String,
    pub artist: String,
    pub position: usize,
    pub match_type: MatchType,
}

#[derive(Debug, Clone, Copy)]
pub enum MatchType {
    Exact,      // Exact title + artist match
    Fuzzy,      // Close match (similarity search)
    LibraryPick, // LLM picked from library sample
}

/// Result of seed selection including the genres used
#[derive(Debug, Clone)]
pub struct SeedSelectionResult {
    pub seeds: Vec<VerifiedSeed>,
    pub genres: Vec<String>,
}

/// Response from LLM for ideal songs
#[derive(Debug, Deserialize)]
struct IdealSongsResponse {
    songs: Vec<IdealSong>,
}

/// Response from LLM for library picks
#[derive(Debug, Deserialize)]
struct LibraryPicksResponse {
    selected_ids: Vec<String>,
    reasoning: String,
}

/// Response from LLM for genre selection
#[derive(Debug, Deserialize)]
struct GenreSelectionResponse {
    relevant_genres: Vec<String>,
    reasoning: String,
}

pub struct SeedSelector {
    anthropic_api_key: String,
    client: reqwest::Client,
    db: PgPool,
}

impl SeedSelector {
    pub fn new(anthropic_api_key: String, db: PgPool) -> Self {
        Self {
            anthropic_api_key,
            client: reqwest::Client::new(),
            db,
        }
    }

    /// Select seed tracks for a query
    ///
    /// Returns seeds with their intended positions in the final playlist
    pub async fn select_seeds(
        &self,
        query: &str,
        seed_count: usize,
        total_playlist_size: usize,
    ) -> Result<Vec<VerifiedSeed>> {
        info!(
            "Selecting {} seeds for query: '{}' (playlist size: {})",
            seed_count, query, total_playlist_size
        );

        // Strategy 1: Ask LLM for ideal songs and verify in library
        let mut seeds = self.try_ideal_songs(query, seed_count * 2).await?;

        // Strategy 2: If not enough, ask LLM to pick from library sample
        if seeds.len() < seed_count {
            let needed = seed_count - seeds.len();
            let exclude_ids: Vec<String> = seeds.iter().map(|s| s.track_id.clone()).collect();
            let more_seeds = self.pick_from_library(query, needed, &exclude_ids).await?;
            seeds.extend(more_seeds);
        }

        // Calculate positions (evenly distributed)
        let interval = if seeds.len() > 1 {
            total_playlist_size / seeds.len()
        } else {
            0
        };

        for (i, seed) in seeds.iter_mut().enumerate() {
            seed.position = i * interval;
        }

        info!(
            "Selected {} seeds: {:?}",
            seeds.len(),
            seeds
                .iter()
                .map(|s| format!("{} - {}", s.artist, s.title))
                .collect::<Vec<_>>()
        );

        Ok(seeds)
    }

    /// Select seeds excluding specific track IDs
    pub async fn select_seeds_excluding(
        &self,
        query: &str,
        seed_count: usize,
        exclude_ids: &[String],
    ) -> Result<Vec<VerifiedSeed>> {
        info!(
            "Selecting {} seeds for query: '{}' (excluding {} tracks)",
            seed_count, query, exclude_ids.len()
        );

        // Just use the library picker with exclusions
        let seeds = self.pick_from_library(query, seed_count, exclude_ids).await?;

        Ok(seeds)
    }

    /// Select seed tracks for a query, also returning the genres determined for the query
    ///
    /// Returns both seeds and the genres that were identified as relevant
    pub async fn select_seeds_with_genres(
        &self,
        query: &str,
        seed_count: usize,
        total_playlist_size: usize,
    ) -> Result<SeedSelectionResult> {
        info!(
            "Selecting {} seeds with genres for query: '{}' (playlist size: {})",
            seed_count, query, total_playlist_size
        );

        // First, determine relevant genres for this query
        let all_genres = self.get_all_genres().await?;
        let genres = if all_genres.is_empty() {
            Vec::new()
        } else {
            self.get_relevant_genres(query, &all_genres).await.unwrap_or_default()
        };

        info!(
            "Determined {} relevant genres for '{}': {:?}",
            genres.len(),
            query,
            genres
        );

        // Now select seeds (this will use the same genre logic internally)
        let seeds = self.select_seeds(query, seed_count, total_playlist_size).await?;

        Ok(SeedSelectionResult { seeds, genres })
    }

    /// Try to find ideal songs in the library
    async fn try_ideal_songs(&self, query: &str, count: usize) -> Result<Vec<VerifiedSeed>> {
        // Ask LLM for ideal songs
        let ideal_songs = self.get_ideal_songs(query, count).await?;

        let mut verified = Vec::new();

        for ideal in ideal_songs {
            // Try exact match first
            if let Some(track) = self.find_exact_match(&ideal.title, &ideal.artist).await? {
                verified.push(VerifiedSeed {
                    track_id: track.id.clone(),
                    title: track.title.clone(),
                    artist: track.artist.clone(),
                    position: 0, // Will be set later
                    match_type: MatchType::Exact,
                });
                debug!(
                    "Found exact match: {} - {}",
                    ideal.artist, ideal.title
                );
                continue;
            }

            // Try fuzzy match
            if let Some(track) = self.find_fuzzy_match(&ideal.title, &ideal.artist).await? {
                verified.push(VerifiedSeed {
                    track_id: track.id.clone(),
                    title: track.title.clone(),
                    artist: track.artist.clone(),
                    position: 0,
                    match_type: MatchType::Fuzzy,
                });
                debug!(
                    "Found fuzzy match for {} - {}: {} - {}",
                    ideal.artist, ideal.title, track.artist, track.title
                );
                continue;
            }

            debug!(
                "No match found for ideal song: {} - {}",
                ideal.artist, ideal.title
            );
        }

        Ok(verified)
    }

    /// Ask LLM for ideal songs that would be perfect for the query
    async fn get_ideal_songs(&self, query: &str, count: usize) -> Result<Vec<IdealSong>> {
        let prompt = format!(
            r#"You are a music expert. For the query "{}", list {} SPECIFIC songs that would be PERFECT examples.

These should be definitive, well-known examples - songs that ANYONE who knows this genre/mood/style would recognize as quintessential.

Focus on:
1. Songs that perfectly embody the requested vibe
2. Different artists to add variety
3. Songs likely to be in a personal music library

Respond with ONLY a JSON object:
{{
  "songs": [
    {{"title": "Song Title", "artist": "Artist Name", "reason": "Why this is perfect"}},
    ...
  ]
}}"#,
            query, count
        );

        let response: IdealSongsResponse = self.call_claude(&prompt).await?;
        Ok(response.songs)
    }

    /// Pick seeds from a sample of the actual library
    ///
    /// Uses a two-stage approach:
    /// 1. First ask LLM which genres are relevant for the query
    /// 2. Sample tracks primarily from those genres
    /// This ensures the LLM gets appropriate options instead of random tracks
    async fn pick_from_library(
        &self,
        query: &str,
        count: usize,
        exclude_ids: &[String],
    ) -> Result<Vec<VerifiedSeed>> {
        // Step 1: Get all unique genres in the library
        let all_genres = self.get_all_genres().await?;

        if all_genres.is_empty() {
            warn!("No genres found in library");
            return Ok(Vec::new());
        }

        // Step 2: Ask LLM which genres are relevant for this query
        let relevant_genres = self.get_relevant_genres(query, &all_genres).await?;

        info!(
            "Selected {} relevant genres for query '{}': {:?}",
            relevant_genres.len(),
            query,
            relevant_genres
        );

        // Step 3: Get a sample that prioritizes relevant genres
        // 80% from relevant genres, 20% random for diversity
        let relevant_sample_size = 160;
        let random_sample_size = 40;

        let mut sample = self
            .get_genre_filtered_sample(&relevant_genres, relevant_sample_size, exclude_ids)
            .await?;

        // Add some random tracks for diversity (may find hidden gems)
        let random_sample = self
            .get_library_sample(random_sample_size, exclude_ids)
            .await?;

        // Merge, avoiding duplicates
        let existing_ids: std::collections::HashSet<String> = sample.iter().map(|t| t.id.clone()).collect();
        for track in random_sample {
            if !existing_ids.contains(&track.id) {
                sample.push(track);
            }
        }

        if sample.is_empty() {
            warn!("No tracks available in library sample");
            return Ok(Vec::new());
        }

        info!(
            "Sampled {} tracks ({} from relevant genres) for query '{}'",
            sample.len(),
            sample.iter().filter(|t| {
                t.genres_vec().iter().any(|g| relevant_genres.contains(g))
            }).count(),
            query
        );

        // Format for LLM
        let track_list: String = sample
            .iter()
            .map(|t| {
                format!(
                    "{}: {} - {} [{}]",
                    t.id,
                    t.artist,
                    t.title,
                    t.genres_vec().join(", ")
                )
            })
            .collect::<Vec<_>>()
            .join("\n");

        let prompt = format!(
            r#"You are selecting seed songs for a radio station. Query: "{}"

AVAILABLE TRACKS IN LIBRARY:
{}

Select EXACTLY {} tracks that are PERFECT examples of "{}".

These seeds will be distributed throughout a playlist, with an AI filling the gaps with sonically similar music. So pick tracks that:
1. Perfectly match the requested vibe
2. Are diverse enough to create interesting transitions
3. Represent different aspects of the request

IMPORTANT: Only return IDs from the list above.

Respond with ONLY a JSON object:
{{
  "selected_ids": ["id1", "id2", ...],
  "reasoning": "Brief explanation of why these tracks were chosen"
}}"#,
            query, track_list, count, query
        );

        let response: LibraryPicksResponse = self.call_claude(&prompt).await?;

        // Convert IDs to VerifiedSeeds
        let mut seeds = Vec::new();
        for id in response.selected_ids {
            if let Some(track) = sample.iter().find(|t| t.id == id) {
                seeds.push(VerifiedSeed {
                    track_id: track.id.clone(),
                    title: track.title.clone(),
                    artist: track.artist.clone(),
                    position: 0,
                    match_type: MatchType::LibraryPick,
                });
            }
        }

        Ok(seeds)
    }

    /// Get all unique genres in the library
    async fn get_all_genres(&self) -> Result<Vec<String>> {
        let genres: Vec<String> = sqlx::query_scalar(
            r#"
            SELECT DISTINCT jsonb_array_elements_text(genres) as genre
            FROM library_index
            WHERE jsonb_array_length(genres) > 0
            ORDER BY genre
            "#,
        )
        .fetch_all(&self.db)
        .await?;

        Ok(genres)
    }

    /// Ask LLM which genres are relevant for a query
    async fn get_relevant_genres(&self, query: &str, all_genres: &[String]) -> Result<Vec<String>> {
        let genre_list = all_genres.join(", ");

        let prompt = format!(
            r#"You are selecting music genres for a playlist. Query: "{}"

AVAILABLE GENRES IN LIBRARY:
{}

Select the genres that would be MOST APPROPRIATE for "{}".

Consider:
- For "sleep" or "relax" queries: ambient, chill, jazz, classical, downtempo
- For "workout" or "energy" queries: rock, hip hop, electronic, metal
- For "study" queries: instrumental, jazz, ambient, classical
- Match the mood and energy level of the request

Select between 5-15 genres that best match the query. Be selective - don't include genres that don't fit.

Respond with ONLY a JSON object:
{{
  "relevant_genres": ["genre1", "genre2", ...],
  "reasoning": "Brief explanation"
}}"#,
            query, genre_list, query
        );

        let response: GenreSelectionResponse = self.call_claude(&prompt).await?;

        // Validate that returned genres actually exist in the library
        let valid_genres: Vec<String> = response
            .relevant_genres
            .into_iter()
            .filter(|g| all_genres.iter().any(|ag| ag.eq_ignore_ascii_case(g)))
            .collect();

        if valid_genres.is_empty() {
            warn!("No valid genres returned by LLM, falling back to all genres");
            // Return a small random subset as fallback
            return Ok(all_genres.iter().take(20).cloned().collect());
        }

        debug!("LLM genre selection reasoning: {}", response.reasoning);
        Ok(valid_genres)
    }

    /// Get a sample of tracks filtered by genres
    async fn get_genre_filtered_sample(
        &self,
        genres: &[String],
        limit: usize,
        exclude_ids: &[String],
    ) -> Result<Vec<SeedTrackInfo>> {
        if genres.is_empty() {
            return self.get_library_sample(limit, exclude_ids).await;
        }

        let tracks = sqlx::query_as::<_, SeedTrackInfo>(
            r#"
            SELECT
                id, title, artist,
                genres::text as genres
            FROM library_index
            WHERE id != ALL($2)
            AND genres ?| $3
            ORDER BY RANDOM()
            LIMIT $1
            "#,
        )
        .bind(limit as i64)
        .bind(exclude_ids)
        .bind(genres)
        .fetch_all(&self.db)
        .await?;

        Ok(tracks)
    }

    /// Find exact title + artist match in library
    async fn find_exact_match(&self, title: &str, artist: &str) -> Result<Option<SeedTrackInfo>> {
        // Use runtime query with ::text cast to avoid pgvector binary protocol issues
        let track = sqlx::query_as::<_, SeedTrackInfo>(
            r#"
            SELECT
                id, title, artist,
                genres::text as genres
            FROM library_index
            WHERE LOWER(title) = LOWER($1)
            AND (LOWER(artist) = LOWER($2) OR LOWER(artist) LIKE LOWER($3))
            LIMIT 1
            "#,
        )
        .bind(title)
        .bind(artist)
        .bind(format!("%{}%", artist))
        .fetch_optional(&self.db)
        .await?;

        Ok(track)
    }

    /// Find fuzzy match using trigram similarity
    async fn find_fuzzy_match(&self, title: &str, artist: &str) -> Result<Option<SeedTrackInfo>> {
        // Use runtime query with ::text cast to avoid pgvector binary protocol issues
        let track = sqlx::query_as::<_, SeedTrackInfo>(
            r#"
            SELECT
                id, title, artist,
                genres::text as genres
            FROM library_index
            WHERE similarity(title, $1) > 0.4
            AND similarity(artist, $2) > 0.4
            ORDER BY similarity(title, $1) + similarity(artist, $2) DESC
            LIMIT 1
            "#,
        )
        .bind(title)
        .bind(artist)
        .fetch_optional(&self.db)
        .await?;

        Ok(track)
    }

    /// Get a representative sample of tracks from the library
    async fn get_library_sample(
        &self,
        limit: usize,
        exclude_ids: &[String],
    ) -> Result<Vec<SeedTrackInfo>> {
        // Use raw query with explicit text cast to avoid sqlx binary protocol issues with pgvector
        let tracks = sqlx::query_as::<_, SeedTrackInfo>(
            r#"
            SELECT
                id, title, artist,
                genres::text as genres
            FROM library_index
            WHERE id != ALL($2)
            ORDER BY RANDOM()
            LIMIT $1
            "#,
        )
        .bind(limit as i64)
        .bind(exclude_ids)
        .fetch_all(&self.db)
        .await?;

        Ok(tracks)
    }

    /// Call Claude API
    async fn call_claude<T: serde::de::DeserializeOwned>(&self, prompt: &str) -> Result<T> {
        let response = self
            .client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &self.anthropic_api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&serde_json::json!({
                "model": "claude-sonnet-4-5-20250929",
                "max_tokens": 4096,
                "messages": [{
                    "role": "user",
                    "content": prompt
                }]
            }))
            .send()
            .await
            .map_err(|e| AppError::ExternalApi(format!("Claude API call failed: {}", e)))?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(AppError::ExternalApi(format!(
                "Claude API error {}: {}",
                status, error_text
            )));
        }

        let response_json: serde_json::Value = response.json().await.map_err(|e| {
            AppError::ExternalApi(format!("Failed to parse Claude response: {}", e))
        })?;

        let content_text = response_json["content"][0]["text"]
            .as_str()
            .ok_or_else(|| AppError::ExternalApi("Invalid Claude response format".to_string()))?;

        // Extract the first complete JSON object from the response
        // This handles cases where Claude adds commentary after the JSON
        let json_text = Self::extract_first_json_object(content_text)
            .ok_or_else(|| AppError::ExternalApi(format!(
                "No valid JSON object found in response: {}",
                &content_text[..content_text.len().min(500)]
            )))?;

        serde_json::from_str(&json_text).map_err(|e| {
            AppError::ExternalApi(format!(
                "Failed to parse Claude JSON: {} | Response: {}",
                e, json_text
            ))
        })
    }

    /// Extract the first complete JSON object from text
    /// Handles markdown code fences and trailing commentary
    fn extract_first_json_object(text: &str) -> Option<String> {
        let text = text.trim();

        // First, try to find JSON within code fences
        if let Some(start) = text.find("```json") {
            let after_fence = &text[start + 7..];
            if let Some(end) = after_fence.find("```") {
                let json_content = after_fence[..end].trim();
                // Verify it's valid JSON by finding the object
                if let Some(obj) = Self::find_json_object(json_content) {
                    return Some(obj);
                }
            }
        }

        // Also check for plain ``` fences
        if let Some(start) = text.find("```") {
            let after_fence = &text[start + 3..];
            // Skip any language identifier on the same line
            let content_start = after_fence.find('\n').map(|i| i + 1).unwrap_or(0);
            let after_lang = &after_fence[content_start..];
            if let Some(end) = after_lang.find("```") {
                let json_content = after_lang[..end].trim();
                if let Some(obj) = Self::find_json_object(json_content) {
                    return Some(obj);
                }
            }
        }

        // No code fences, try to find raw JSON object
        Self::find_json_object(text)
    }

    /// Find the first complete JSON object in text by matching braces
    fn find_json_object(text: &str) -> Option<String> {
        let start = text.find('{')?;
        let chars: Vec<char> = text[start..].chars().collect();

        let mut depth = 0;
        let mut in_string = false;
        let mut escape_next = false;

        for (i, &ch) in chars.iter().enumerate() {
            if escape_next {
                escape_next = false;
                continue;
            }

            match ch {
                '\\' if in_string => escape_next = true,
                '"' => in_string = !in_string,
                '{' if !in_string => depth += 1,
                '}' if !in_string => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(chars[..=i].iter().collect());
                    }
                }
                _ => {}
            }
        }

        None
    }
}
