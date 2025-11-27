use crate::error::{AppError, Result};
use crate::models::{
    CurationProgress, LibraryStats, LibraryTrack, QueryAnalysisRequest, QueryAnalysisResult,
    QueryFilters, TrackSelectionRequest, TrackSelectionResult,
};
use sqlx::PgPool;
use std::collections::HashMap;
use tokio::sync::mpsc;
use tracing::{info, warn};

/// Multi-layered AI music curator
/// Uses Claude to intelligently analyze queries and select tracks
pub struct AiCurator {
    anthropic_api_key: String,
    client: reqwest::Client,
    db: PgPool,
}

impl AiCurator {
    pub fn new(anthropic_api_key: String, db: PgPool) -> Self {
        Self {
            anthropic_api_key,
            client: reqwest::Client::new(),
            db,
        }
    }

    /// Main entry point: Curate tracks based on natural language query
    /// This implements the 3-layer AI approach:
    /// 1. Get library context
    /// 2. Analyze query to extract filters
    /// 3. Select and rank specific tracks
    pub async fn curate_tracks(&self, query: String, limit: usize) -> Result<Vec<String>> {
        // Use the progress version but discard the receiver
        let (tx, _rx) = mpsc::channel(10);
        self.curate_tracks_with_progress(query, limit, tx).await
    }

    /// Curate tracks with progress updates via the provided channel
    pub async fn curate_tracks_with_progress(
        &self,
        query: String,
        limit: usize,
        progress_tx: mpsc::Sender<CurationProgress>,
    ) -> Result<Vec<String>> {
        info!("Curating tracks for query: {}", query);

        // Helper to send progress (ignoring errors if receiver dropped)
        let send_progress = |p: CurationProgress| {
            let tx = progress_tx.clone();
            async move { let _ = tx.send(p).await; }
        };

        send_progress(CurationProgress::Started {
            query: query.clone(),
            message: "Starting AI curation...".to_string(),
        }).await;

        // Check cache first
        send_progress(CurationProgress::CheckingCache {
            message: "Checking for cached analysis...".to_string(),
        }).await;

        let query_hash = format!("{:x}", md5::compute(&query));
        if let Some(cached) = self.get_cached_query(&query_hash).await? {
            info!("Using cached query analysis");

            send_progress(CurationProgress::SearchingTracks {
                message: "Found cached analysis, searching for matching tracks...".to_string(),
                filters_applied: Some(serde_json::to_value(&cached.filters).unwrap_or_default()),
            }).await;

            // Get matching tracks using cached filters (cap to avoid API rate limits)
            let max_candidates = 100;  // Keep low to avoid rate limits
            let tracks = self.get_matching_tracks(&cached.filters, max_candidates).await?;

            // If we found tracks with cached filters, use them (skip Layer 2)
            if !tracks.is_empty() {
                let actual_limit = std::cmp::min(limit, tracks.len());

                send_progress(CurationProgress::AiSelectingTracks {
                    message: "AI is selecting the best tracks...".to_string(),
                    candidate_count: tracks.len(),
                    thinking: Some("Analyzing track relevance to your query".to_string()),
                }).await;

                // Use AI for final selection with original query for strict matching
                let result = self.ai_select_tracks(&query, tracks, actual_limit).await?;

                send_progress(CurationProgress::Completed {
                    message: "Curation complete!".to_string(),
                    tracks_selected: result.len(),
                    reasoning: None,
                }).await;

                return Ok(result);
            }
        }

        // Layer 1: Get library context
        send_progress(CurationProgress::AnalyzingLibrary {
            message: "Analyzing your music library...".to_string(),
        }).await;

        let library_stats = self.get_library_context().await?;

        // Layer 2: Analyze query with AI to extract filters
        send_progress(CurationProgress::AiAnalyzingQuery {
            message: "AI is analyzing your request...".to_string(),
            thinking: Some(format!(
                "Understanding '{}' and finding matching genres/moods",
                query
            )),
        }).await;

        let analysis = self.analyze_query_with_ai(query.clone(), library_stats).await?;

        // Cache the analysis
        self.cache_query_analysis(&query_hash, &query, &analysis).await?;

        // Get candidate tracks matching the filters
        send_progress(CurationProgress::SearchingTracks {
            message: format!("Searching for {} tracks...", analysis.semantic_intent),
            filters_applied: Some(serde_json::to_value(&analysis.filters).unwrap_or_default()),
        }).await;

        // Cap candidates to avoid hitting API rate limits (100 track descriptions max)
        let max_candidates = 100;
        let candidate_tracks = self
            .get_matching_tracks(&analysis.filters, max_candidates)
            .await?;

        info!(
            "Found {} candidate tracks matching filters",
            candidate_tracks.len()
        );

        if candidate_tracks.is_empty() {
            warn!("No tracks found matching filters, using fallback");
            send_progress(CurationProgress::SearchingTracks {
                message: "No exact matches found, using broader search...".to_string(),
                filters_applied: None,
            }).await;
            // Fallback: just get random tracks from library
            let result = self.get_random_tracks(limit).await?;

            send_progress(CurationProgress::Completed {
                message: "Curation complete (using random selection)".to_string(),
                tracks_selected: result.len(),
                reasoning: Some("No exact matches found for your query".to_string()),
            }).await;

            return Ok(result);
        }

        // Layer 3: Use AI to select and rank the best tracks (with strict matching)
        send_progress(CurationProgress::AiSelectingTracks {
            message: "AI is selecting and validating tracks...".to_string(),
            candidate_count: candidate_tracks.len(),
            thinking: Some(format!(
                "Strictly filtering {} candidates to find genuine matches for '{}'",
                candidate_tracks.len(),
                query
            )),
        }).await;

        let result = self.ai_select_tracks(&query, candidate_tracks, limit).await?;

        send_progress(CurationProgress::Completed {
            message: "Curation complete!".to_string(),
            tracks_selected: result.len(),
            reasoning: Some(format!(
                "Selected {} tracks that genuinely match your request",
                result.len()
            )),
        }).await;

        Ok(result)
    }

    async fn get_library_context(&self) -> Result<LibraryStats> {
        let stats = sqlx::query!(
            r#"
            SELECT
                total_tracks, total_artists, total_albums,
                genre_distribution, artist_distribution,
                earliest_year, latest_year, year_distribution,
                mood_distribution, avg_energy, avg_tempo, avg_valence,
                song_type_distribution, total_ai_analyzed, ai_analysis_percentage,
                computed_at
            FROM library_stats
            ORDER BY computed_at DESC
            LIMIT 1
            "#
        )
        .fetch_one(&self.db)
        .await?;

        Ok(LibraryStats {
            total_tracks: stats.total_tracks,
            total_artists: stats.total_artists,
            total_albums: stats.total_albums,
            genre_distribution: stats.genre_distribution,
            artist_distribution: stats.artist_distribution,
            earliest_year: stats.earliest_year,
            latest_year: stats.latest_year,
            year_distribution: stats.year_distribution,
            mood_distribution: stats.mood_distribution,
            avg_energy: stats.avg_energy,
            avg_tempo: stats.avg_tempo,
            avg_valence: stats.avg_valence,
            song_type_distribution: stats.song_type_distribution,
            total_ai_analyzed: stats.total_ai_analyzed,
            ai_analysis_percentage: stats.ai_analysis_percentage,
            computed_at: stats.computed_at,
        })
    }

    async fn analyze_query_with_ai(
        &self,
        query: String,
        library_context: LibraryStats,
    ) -> Result<QueryAnalysisResult> {
        info!("Analyzing query with AI (Layer 2)");

        // Format library context for AI - show more genres so AI has better options
        let top_genres: Vec<String> = library_context
            .genre_distribution
            .as_object()
            .map(|obj| obj.keys().take(100).map(|k| k.clone()).collect())
            .unwrap_or_default();

        let top_artists: Vec<String> = library_context
            .artist_distribution
            .as_object()
            .map(|obj| obj.keys().take(30).map(|k| k.clone()).collect())
            .unwrap_or_default();

        let available_moods: Vec<String> = library_context
            .mood_distribution
            .as_object()
            .map(|obj| obj.keys().map(|k| k.clone()).collect())
            .unwrap_or_default();

        let prompt = format!(
            r#"You are a music library curator. Analyze this query and determine what the user wants to hear.

USER QUERY: "{}"

LIBRARY CONTEXT:
- Total tracks: {}
- Total artists: {}
- Year range: {} to {}
- Available genres (USE ONLY THESE): {}
- Top artists: {}
- Available moods: {}
- Average energy: {:.2}
- Average tempo: {:.1} BPM
- Average valence (happiness): {:.2}

CRITICAL INSTRUCTIONS:
1. For genres, ONLY use genres from the "Available genres" list above - don't invent genres!
2. Include BROAD genre categories in addition to specific ones (e.g., for hip hop: include "Hip Hop", "Rap", "Rap/Hip Hop" AND specific subgenres like "East Coast Hip Hop")
3. Keep filters LOOSE - it's better to have too many matches (we'll filter later) than too few
4. Only use year_range if the query specifically mentions a time period
5. Only use energy_range if the query specifically mentions energy/intensity
6. Leave filters as null if not relevant to the query

Think about what genres from the available list would match "{}"

Respond with ONLY a JSON object:
{{
  "semantic_intent": "Brief description of what the user wants",
  "filters": {{
    "genres": ["genre1", "genre2"] or null,
    "artists": ["artist1", "artist2"] or null,
    "moods": ["mood1", "mood2"] or null,
    "themes": ["theme1"] or null,
    "song_types": ["type1"] or null,
    "year_range": [start_year, end_year] or null,
    "energy_range": [min, max] or null,
    "tempo_range": [min_bpm, max_bpm] or null,
    "valence_range": [min, max] or null,
    "min_rating": null
  }},
  "confidence": 0.85
}}"#,
            query,
            library_context.total_tracks,
            library_context.total_artists,
            library_context.earliest_year.unwrap_or(1950),
            library_context.latest_year.unwrap_or(2024),
            top_genres.join(", "),
            top_artists.join(", "),
            available_moods.join(", "),
            library_context.avg_energy.unwrap_or(0.5),
            library_context.avg_tempo.unwrap_or(120.0),
            library_context.avg_valence.unwrap_or(0.5),
            query,  // Second instance for "Think about what genres" line
        );

        let analysis = self.call_claude(&prompt).await?;

        Ok(analysis)
    }

    async fn get_matching_tracks(
        &self,
        filters: &QueryFilters,
        limit: usize,
    ) -> Result<Vec<LibraryTrack>> {
        let mut query_parts = vec!["SELECT * FROM library_index WHERE 1=1".to_string()];
        let mut bind_index = 1;

        // Build dynamic SQL query based on filters
        if let Some(genres) = &filters.genres {
            if !genres.is_empty() {
                query_parts.push(format!(
                    "AND genres ?| ARRAY[{}]",
                    genres
                        .iter()
                        .map(|g| format!("'{}'", g))
                        .collect::<Vec<_>>()
                        .join(",")
                ));
            }
        }

        if let Some(moods) = &filters.moods {
            if !moods.is_empty() {
                query_parts.push(format!(
                    "AND mood_tags ?| ARRAY[{}]",
                    moods
                        .iter()
                        .map(|m| format!("'{}'", m))
                        .collect::<Vec<_>>()
                        .join(",")
                ));
            }
        }

        if let Some((min_energy, max_energy)) = filters.energy_range {
            // Include tracks with NULL energy_level (not yet analyzed)
            query_parts.push(format!(
                "AND (energy_level IS NULL OR energy_level BETWEEN {} AND {})",
                min_energy, max_energy
            ));
        }

        if let Some((min_year, max_year)) = filters.year_range {
            // Include tracks with NULL year
            query_parts.push(format!(
                "AND (year IS NULL OR year BETWEEN {} AND {})",
                min_year, max_year
            ));
        }

        if let Some(min_rating) = filters.min_rating {
            // Include tracks with NULL rating
            query_parts.push(format!(
                "AND (avg_rating IS NULL OR avg_rating >= {})",
                min_rating
            ));
        }

        query_parts.push(format!("LIMIT {}", limit));

        let query_str = query_parts.join(" ");

        info!("Executing track query: {}", query_str);

        // Execute the query
        let tracks = sqlx::query_as::<_, LibraryTrack>(&query_str)
            .fetch_all(&self.db)
            .await?;

        Ok(tracks)
    }

    async fn ai_select_tracks(
        &self,
        original_query: &str,
        candidates: Vec<LibraryTrack>,
        limit: usize,
    ) -> Result<Vec<String>> {
        info!(
            "Using AI to select best {} tracks from {} candidates (Layer 3)",
            limit,
            candidates.len()
        );

        // Prepare candidate list for AI
        let candidate_descriptions: Vec<String> = candidates
            .iter()
            .map(|t| {
                format!(
                    "ID: {} | \"{}\" by {} ({}) | Genres: {} | Moods: {} | Energy: {:.2} | Year: {}",
                    t.id,
                    t.title,
                    t.artist,
                    t.album,
                    t.genres.join(", "),
                    t.mood_tags.join(", "),
                    t.energy_level.unwrap_or(0.5),
                    t.year.unwrap_or(0)
                )
            })
            .collect();

        let prompt = format!(
            r#"You are a strict music curator. Your job is to select ONLY tracks that genuinely match the user's request.

USER'S REQUEST: "{}"

CANDIDATE TRACKS:
{}

CRITICAL INSTRUCTIONS:
1. REJECT any track that doesn't fit the requested vibe, genre, or mood - even if it's a good song
2. A jazz track does NOT belong in a hip hop playlist
3. A pop song does NOT belong in a metal playlist
4. An upbeat song does NOT belong in a dark/sad playlist
5. Be STRICT - it's better to return fewer tracks than to include ones that break the vibe
6. Only select tracks where the artist, genre, and mood genuinely match the request
7. Use your knowledge of music to identify mismatches (e.g., Art Tatum is jazz, not hip hop)

Select up to {} tracks that GENUINELY match "{}".
If fewer than {} tracks truly match, return fewer. Quality over quantity.

Respond with ONLY a JSON object:
{{
  "selected_tracks": ["track_id_1", "track_id_2", ...],
  "scores": [0.95, 0.88, ...],
  "reasoning": "Brief explanation of why these tracks match and what was rejected"
}}"#,
            original_query,
            candidate_descriptions.join("\n"),
            limit,
            original_query,
            limit
        );

        let result: TrackSelectionResult = self.call_claude(&prompt).await?;

        Ok(result.selected_tracks)
    }

    async fn call_claude<T: serde::de::DeserializeOwned>(&self, prompt: &str) -> Result<T> {
        let response = self
            .client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &self.anthropic_api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&serde_json::json!({
                "model": "claude-sonnet-4-5-20250929",
                "max_tokens": 8192,  // Enough for ~150 track IDs + scores in response
                "messages": [{
                    "role": "user",
                    "content": prompt
                }]
            }))
            .send()
            .await
            .map_err(|e| AppError::ExternalApi(format!("Failed to call Claude API: {}", e)))?;

        // Check HTTP status code
        let status = response.status();
        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
            return Err(AppError::ExternalApi(format!(
                "Claude API returned error status {}: {}",
                status, error_text
            )));
        }

        let response_json: serde_json::Value = response
            .json()
            .await
            .map_err(|e| AppError::ExternalApi(format!("Failed to parse Claude response: {}", e)))?;

        // Extract the text content from Claude's response
        let content_text = response_json["content"][0]["text"]
            .as_str()
            .ok_or_else(|| AppError::ExternalApi("Invalid response format from Claude".to_string()))?;

        // Strip markdown code fences if present (Claude sometimes wraps JSON in ```json ... ```)
        let json_text = content_text
            .trim()
            .strip_prefix("```json")
            .or_else(|| content_text.trim().strip_prefix("```"))
            .map(|s| s.strip_suffix("```").unwrap_or(s))
            .unwrap_or(content_text)
            .trim();

        // Parse the JSON from the text content
        let result: T = serde_json::from_str(json_text)
            .map_err(|e| AppError::ExternalApi(format!("Failed to parse Claude JSON response: {} | Response was: {}", e, json_text)))?;

        Ok(result)
    }

    async fn get_cached_query(&self, query_hash: &str) -> Result<Option<QueryAnalysisResult>> {
        let cached = sqlx::query!(
            r#"
            SELECT analyzed_filters, semantic_intent
            FROM ai_query_cache
            WHERE query_hash = $1
            "#,
            query_hash
        )
        .fetch_optional(&self.db)
        .await?;

        if let Some(record) = cached {
            // Update last_used and use_count
            sqlx::query!(
                r#"
                UPDATE ai_query_cache
                SET last_used = NOW(), use_count = use_count + 1
                WHERE query_hash = $1
                "#,
                query_hash
            )
            .execute(&self.db)
            .await?;

            let filters: QueryFilters = serde_json::from_value(record.analyzed_filters)?;

            Ok(Some(QueryAnalysisResult {
                semantic_intent: record.semantic_intent.unwrap_or_default(),
                filters,
                confidence: 1.0,
            }))
        } else {
            Ok(None)
        }
    }

    async fn cache_query_analysis(
        &self,
        query_hash: &str,
        original_query: &str,
        analysis: &QueryAnalysisResult,
    ) -> Result<()> {
        let filters_json = serde_json::to_value(&analysis.filters)?;

        sqlx::query!(
            r#"
            INSERT INTO ai_query_cache (query_hash, original_query, analyzed_filters, semantic_intent, ai_model_version)
            VALUES ($1, $2, $3, $4, 'claude-sonnet-4-5-20250929')
            ON CONFLICT (query_hash) DO UPDATE SET
                last_used = NOW(),
                use_count = ai_query_cache.use_count + 1
            "#,
            query_hash,
            original_query,
            filters_json,
            analysis.semantic_intent
        )
        .execute(&self.db)
        .await?;

        Ok(())
    }

    async fn get_random_tracks(&self, limit: usize) -> Result<Vec<String>> {
        let tracks = sqlx::query_scalar!(
            r#"
            SELECT id
            FROM library_index
            ORDER BY RANDOM()
            LIMIT $1
            "#,
            limit as i64
        )
        .fetch_all(&self.db)
        .await?;

        Ok(tracks)
    }
}
