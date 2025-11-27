use crate::error::{AppError, Result};
use crate::models::{
    LibraryStats, LibraryTrack, QueryAnalysisRequest, QueryAnalysisResult, QueryFilters,
    TrackSelectionRequest, TrackSelectionResult,
};
use sqlx::PgPool;
use std::collections::HashMap;
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
        info!("Curating tracks for query: {}", query);

        // Check cache first
        let query_hash = format!("{:x}", md5::compute(&query));
        if let Some(cached) = self.get_cached_query(&query_hash).await? {
            info!("Using cached query analysis");

            // Get matching tracks using cached filters
            let tracks = self.get_matching_tracks(&cached.filters, limit * 3).await?;

            if tracks.len() >= limit {
                // Use AI for final selection
                return self.ai_select_tracks(&cached.filters, tracks, limit).await;
            }
        }

        // Layer 1: Get library context
        let library_stats = self.get_library_context().await?;

        // Layer 2: Analyze query with AI to extract filters
        let analysis = self.analyze_query_with_ai(query.clone(), library_stats).await?;

        // Cache the analysis
        self.cache_query_analysis(&query_hash, &query, &analysis).await?;

        // Get candidate tracks matching the filters
        let candidate_tracks = self
            .get_matching_tracks(&analysis.filters, limit * 3)
            .await?;

        info!(
            "Found {} candidate tracks matching filters",
            candidate_tracks.len()
        );

        if candidate_tracks.is_empty() {
            warn!("No tracks found matching filters, using fallback");
            // Fallback: just get random tracks from library
            return self.get_random_tracks(limit).await;
        }

        // Layer 3: Use AI to select and rank the best tracks
        self.ai_select_tracks(&analysis.filters, candidate_tracks, limit)
            .await
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

        // Format library context for AI
        let top_genres: Vec<String> = library_context
            .genre_distribution
            .as_object()
            .map(|obj| obj.keys().take(20).map(|k| k.clone()).collect())
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
- Top genres: {}
- Top artists: {}
- Available moods: {}
- Average energy: {:.2}
- Average tempo: {:.1} BPM
- Average valence (happiness): {:.2}

Based on the user's query and what's actually in their library, extract filters that would match appropriate songs.
Think about:
1. What genres would match this query given the available genres?
2. Which specific artists in the library might fit?
3. What mood tags apply?
4. What energy level range (0.0-1.0)?
5. What year range makes sense?
6. Any tempo preferences?
7. Valence (happiness) range?

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
    "min_rating": 3.5 or null
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
            query_parts.push(format!(
                "AND energy_level BETWEEN {} AND {}",
                min_energy, max_energy
            ));
        }

        if let Some((min_year, max_year)) = filters.year_range {
            query_parts.push(format!("AND year BETWEEN {} AND {}", min_year, max_year));
        }

        if let Some(min_rating) = filters.min_rating {
            query_parts.push(format!("AND avg_rating >= {}", min_rating));
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
        filters: &QueryFilters,
        candidates: Vec<LibraryTrack>,
        limit: usize,
    ) -> Result<Vec<String>> {
        info!(
            "Using AI to select best {} tracks from {} candidates (Layer 3)",
            limit,
            candidates.len()
        );

        if candidates.len() <= limit {
            return Ok(candidates.into_iter().map(|t| t.id).collect());
        }

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
            r#"You are selecting the best {} tracks from a list of candidates for a user's radio station.

FILTER CRITERIA:
{}

CANDIDATE TRACKS:
{}

Select the {} best tracks that:
1. Match the filter criteria most closely
2. Provide good variety and flow
3. Are well-rated if ratings exist
4. Create an engaging listening experience

Respond with ONLY a JSON object:
{{
  "selected_tracks": ["track_id_1", "track_id_2", ...],
  "scores": [0.95, 0.88, ...],
  "reasoning": "Brief explanation of selection strategy"
}}"#,
            limit,
            serde_json::to_string_pretty(filters).unwrap_or_default(),
            candidate_descriptions.join("\n"),
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
                "model": "claude-3-5-sonnet-20241022",
                "max_tokens": 2048,
                "messages": [{
                    "role": "user",
                    "content": prompt
                }]
            }))
            .send()
            .await
            .map_err(|e| AppError::ExternalApi(format!("Failed to call Claude API: {}", e)))?;

        let response_json: serde_json::Value = response
            .json()
            .await
            .map_err(|e| AppError::ExternalApi(format!("Failed to parse Claude response: {}", e)))?;

        // Extract the text content from Claude's response
        let content_text = response_json["content"][0]["text"]
            .as_str()
            .ok_or_else(|| AppError::ExternalApi("Invalid response format from Claude".to_string()))?;

        // Parse the JSON from the text content
        let result: T = serde_json::from_str(content_text)
            .map_err(|e| AppError::ExternalApi(format!("Failed to parse Claude JSON response: {} | Response was: {}", e, content_text)))?;

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
            VALUES ($1, $2, $3, $4, 'claude-3-5-sonnet-20241022')
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
