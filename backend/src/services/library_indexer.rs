#![allow(dead_code)]

use crate::error::{AppError, Result};
use crate::models::{
    LibraryTrack, LibrarySyncStatus, TrackAnalysisRequest, TrackAnalysisResult,
};
use crate::services::navidrome::NavidromeClient;
use sqlx::PgPool;
use std::sync::Arc;
use tokio::sync::Semaphore;
use tracing::{error, info, warn};

pub struct LibraryIndexer {
    db: PgPool,
    navidrome_client: Arc<NavidromeClient>,
    ai_analyzer: Option<Arc<TrackAnalyzer>>,
    max_concurrent_ai_calls: usize,
}

impl LibraryIndexer {
    pub fn new(
        db: PgPool,
        navidrome_client: Arc<NavidromeClient>,
        ai_analyzer: Option<Arc<TrackAnalyzer>>,
    ) -> Self {
        Self {
            db,
            navidrome_client,
            ai_analyzer,
            max_concurrent_ai_calls: 5, // Process 5 tracks concurrently
        }
    }

    /// Perform a full sync of the library from Navidrome
    /// If progress_tx is provided, sends progress updates via the channel
    pub async fn sync_full(&self, progress_tx: Option<tokio::sync::broadcast::Sender<crate::models::SyncProgress>>) -> Result<()> {
        info!("Starting full library sync from Navidrome");

        // Check if sync is already in progress
        let status = self.get_sync_status().await?;
        if status.sync_in_progress {
            warn!("Sync already in progress, skipping");
            return Ok(());
        }

        // Mark sync as in progress
        self.update_sync_status(true, None).await?;

        match self.perform_full_sync(progress_tx.clone()).await {
            Ok(total_tracks) => {
                info!("Full library sync completed successfully");
                self.update_sync_status(false, None).await?;

                // Send completed event
                if let Some(tx) = &progress_tx {
                    let _ = tx.send(crate::models::SyncProgress::Completed {
                        total_tracks,
                        message: format!("Library sync completed successfully. {} tracks synced.", total_tracks),
                    });
                }

                // Update stats in background (don't block completion)
                info!("Library sync complete. Stats computation will run in background.");
                let db_clone = self.db.clone();
                let progress_tx_clone = progress_tx;
                tokio::spawn(async move {
                    info!("Computing library statistics...");

                    // Send computing stats event
                    if let Some(tx) = &progress_tx_clone {
                        let _ = tx.send(crate::models::SyncProgress::ComputingStats {
                            message: "Computing library statistics...".to_string(),
                        });
                    }

                    match sqlx::query!("SELECT update_library_stats()")
                        .execute(&db_clone)
                        .await
                    {
                        Ok(_) => info!("Library statistics updated successfully"),
                        Err(e) => error!("Failed to update library statistics: {}", e),
                    }
                });

                Ok(())
            }
            Err(e) => {
                error!("Full library sync failed: {}", e);
                self.update_sync_status(false, Some(e.to_string())).await?;

                // Send error event
                if let Some(tx) = &progress_tx {
                    let _ = tx.send(crate::models::SyncProgress::Error {
                        message: format!("Sync failed: {}", e),
                    });
                }

                Err(e)
            }
        }
    }

    async fn perform_full_sync(&self, progress_tx: Option<tokio::sync::broadcast::Sender<crate::models::SyncProgress>>) -> Result<usize> {
        // Use paginated API to get ALL songs from Navidrome
        let page_size = 500;
        let mut offset = 0;
        let mut total_synced = 0;
        let mut total_count = 0;

        info!("Starting full library sync using paginated API");

        loop {
            // Send fetching event
            if let Some(tx) = &progress_tx {
                let _ = tx.send(crate::models::SyncProgress::Fetching {
                    iteration: (offset / page_size) + 1,
                    message: format!("Fetching songs {} - {} from Navidrome...", offset, offset + page_size),
                });
            }

            let (tracks, count) = match self.navidrome_client.get_all_songs_paginated(page_size, offset).await {
                Ok(result) => result,
                Err(e) => {
                    warn!("Failed to fetch tracks at offset {}: {}", offset, e);
                    break;
                }
            };

            if total_count == 0 {
                total_count = count;
                info!("Navidrome reports {} total songs", total_count);

                // Update total in database
                sqlx::query!(
                    "UPDATE library_sync_status SET total_tracks_in_navidrome = $1 WHERE id = 1",
                    total_count as i32
                )
                .execute(&self.db)
                .await?;
            }

            if tracks.is_empty() {
                info!("No more tracks to fetch");
                break;
            }

            let batch_count = tracks.len();

            // Upsert all tracks
            for track in &tracks {
                if let Err(e) = self.upsert_track(track).await {
                    warn!("Failed to upsert track {}: {}", track.id, e);
                } else {
                    total_synced += 1;
                }
            }

            info!(
                "Synced {} tracks (offset {}, batch size {}), {} / {} total",
                batch_count, offset, page_size, total_synced, total_count
            );

            // Send processing event
            if let Some(tx) = &progress_tx {
                let _ = tx.send(crate::models::SyncProgress::Processing {
                    current: total_synced,
                    total: total_count,
                    new_tracks: batch_count,
                    message: format!("Synced {} / {} tracks", total_synced, total_count),
                });
            }

            sqlx::query!(
                "UPDATE library_sync_status SET tracks_synced = $1 WHERE id = 1",
                total_synced as i32
            )
            .execute(&self.db)
            .await?;

            offset += page_size;

            // Stop if we've fetched all tracks
            if offset >= total_count {
                break;
            }
        }

        info!("Synced {} total tracks", total_synced);

        // Update sync timestamp
        sqlx::query!(
            "UPDATE library_sync_status SET last_full_sync = NOW(), total_tracks_in_navidrome = $1 WHERE id = 1",
            total_synced as i32
        )
        .execute(&self.db)
        .await?;

        Ok(total_synced)
    }

    async fn upsert_track(&self, track: &crate::models::Track) -> Result<()> {
        let genres_json = serde_json::to_value(&track.genre)?;

        sqlx::query!(
            r#"
            INSERT INTO library_index (
                id, title, artist, album, year, duration, genres, path, last_synced
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, NOW())
            ON CONFLICT (id) DO UPDATE SET
                title = EXCLUDED.title,
                artist = EXCLUDED.artist,
                album = EXCLUDED.album,
                year = EXCLUDED.year,
                duration = EXCLUDED.duration,
                genres = EXCLUDED.genres,
                path = EXCLUDED.path,
                last_synced = NOW()
            "#,
            track.id,
            track.title,
            track.artist,
            track.album,
            track.year,
            track.duration,
            genres_json,
            track.path
        )
        .execute(&self.db)
        .await?;

        Ok(())
    }

    /// Perform AI analysis on unanalyzed tracks
    pub async fn analyze_unanalyzed_tracks(&self, limit: usize) -> Result<usize> {
        if self.ai_analyzer.is_none() {
            warn!("AI analyzer not configured, skipping track analysis");
            return Ok(0);
        }

        let analyzer = self.ai_analyzer.as_ref().unwrap();

        // Get unanalyzed tracks
        let tracks = sqlx::query_as!(
            LibraryTrack,
            r#"
            SELECT
                id, title, artist, album, album_artist, composer, year, duration,
                genres as "genres!: _",
                mood_tags as "mood_tags!: _",
                energy_level, danceability, valence, tempo,
                song_type as "song_type!: _",
                themes as "themes!: _",
                acousticness, instrumentalness,
                play_count, skip_count, last_played,
                user_rating, avg_rating, rating_count,
                musicbrainz_id, rym_rating, rym_rating_count,
                lastfm_playcount, lastfm_listeners,
                ai_analyzed, ai_analysis_version, last_synced, last_ai_analysis
            FROM library_index
            WHERE ai_analyzed = false
            LIMIT $1
            "#,
            limit as i64
        )
        .fetch_all(&self.db)
        .await?;

        info!("Analyzing {} unanalyzed tracks", tracks.len());

        let semaphore = Arc::new(Semaphore::new(self.max_concurrent_ai_calls));
        let mut handles = vec![];

        for track in tracks {
            let analyzer = Arc::clone(analyzer);
            let db = self.db.clone();
            let permit = Arc::clone(&semaphore);

            let handle = tokio::spawn(async move {
                let _permit = permit.acquire().await.unwrap();

                let request = TrackAnalysisRequest {
                    track_id: track.id.clone(),
                    title: track.title.clone(),
                    artist: track.artist.clone(),
                    album: track.album.clone(),
                    genres: track.genres.clone(),
                    year: track.year,
                };

                match analyzer.analyze_track(request).await {
                    Ok(analysis) => {
                        if let Err(e) = Self::update_track_analysis(&db, &track.id, analysis).await
                        {
                            warn!("Failed to update analysis for track {}: {}", track.id, e);
                        }
                    }
                    Err(e) => {
                        warn!("Failed to analyze track {}: {}", track.id, e);
                    }
                }
            });

            handles.push(handle);
        }

        // Wait for all analysis tasks to complete
        for handle in handles {
            let _ = handle.await;
        }

        // Update stats
        let analyzed_count = sqlx::query_scalar!(
            "SELECT COUNT(*) as count FROM library_index WHERE ai_analyzed = true"
        )
        .fetch_one(&self.db)
        .await?
        .unwrap_or(0);

        sqlx::query!(
            "UPDATE library_sync_status SET tracks_analyzed = $1 WHERE id = 1",
            analyzed_count as i32
        )
        .execute(&self.db)
        .await?;

        info!("Completed AI analysis");
        Ok(limit)
    }

    async fn update_track_analysis(
        db: &PgPool,
        track_id: &str,
        analysis: TrackAnalysisResult,
    ) -> Result<()> {
        let mood_tags_json = serde_json::to_value(&analysis.mood_tags)?;
        let song_type_json = serde_json::to_value(&analysis.song_type)?;
        let themes_json = serde_json::to_value(&analysis.themes)?;

        sqlx::query!(
            r#"
            UPDATE library_index SET
                mood_tags = $2,
                energy_level = $3,
                danceability = $4,
                valence = $5,
                song_type = $6,
                themes = $7,
                acousticness = $8,
                instrumentalness = $9,
                ai_analyzed = true,
                last_ai_analysis = NOW()
            WHERE id = $1
            "#,
            track_id,
            mood_tags_json,
            analysis.energy_level,
            analysis.danceability,
            analysis.valence,
            song_type_json,
            themes_json,
            analysis.acousticness,
            analysis.instrumentalness
        )
        .execute(db)
        .await?;

        Ok(())
    }

    pub async fn get_sync_status(&self) -> Result<LibrarySyncStatus> {
        let status = sqlx::query_as!(
            LibrarySyncStatus,
            r#"
            SELECT
                id, last_full_sync, last_incremental_sync, sync_in_progress,
                total_tracks_in_navidrome, tracks_synced, tracks_analyzed,
                last_sync_error, last_sync_error_at, navidrome_version,
                current_ai_version, updated_at
            FROM library_sync_status
            WHERE id = 1
            "#
        )
        .fetch_one(&self.db)
        .await?;

        Ok(status)
    }

    async fn update_sync_status(&self, in_progress: bool, error: Option<String>) -> Result<()> {
        if let Some(err_msg) = error {
            sqlx::query!(
                "UPDATE library_sync_status SET sync_in_progress = $1, last_sync_error = $2, last_sync_error_at = NOW(), updated_at = NOW() WHERE id = 1",
                in_progress,
                err_msg
            )
            .execute(&self.db)
            .await?;
        } else {
            sqlx::query!(
                "UPDATE library_sync_status SET sync_in_progress = $1, updated_at = NOW() WHERE id = 1",
                in_progress
            )
            .execute(&self.db)
            .await?;
        }

        Ok(())
    }

    async fn update_library_stats(&self) -> Result<()> {
        info!("Updating library statistics");

        sqlx::query!("SELECT update_library_stats()")
            .execute(&self.db)
            .await?;

        Ok(())
    }

    /// Get current library statistics
    pub async fn get_library_stats(&self) -> Result<crate::models::LibraryStats> {
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

        Ok(crate::models::LibraryStats {
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
}

/// AI-powered track analyzer using Claude
pub struct TrackAnalyzer {
    anthropic_api_key: String,
    client: reqwest::Client,
}

impl TrackAnalyzer {
    pub fn new(anthropic_api_key: String) -> Self {
        Self {
            anthropic_api_key,
            client: reqwest::Client::new(),
        }
    }

    pub async fn analyze_track(&self, request: TrackAnalysisRequest) -> Result<TrackAnalysisResult> {
        let prompt = format!(
            r#"Analyze this music track and provide detailed metadata:

Track: "{}" by {}
Album: {}
Genres: {}
Year: {}

Please analyze this track and provide:
1. mood_tags: List of 3-5 mood descriptors (e.g., "energetic", "melancholic", "upbeat", "chill", "aggressive")
2. energy_level: Float 0.0-1.0 (0 = very calm, 1 = very energetic)
3. danceability: Float 0.0-1.0 (0 = not danceable, 1 = very danceable)
4. valence: Float 0.0-1.0 (0 = sad/dark, 1 = happy/bright)
5. song_type: List of types (e.g., "ballad", "anthem", "instrumental", "dance")
6. themes: List of themes (e.g., "love", "loss", "celebration", "introspection")
7. acousticness: Float 0.0-1.0 (0 = electronic, 1 = acoustic)
8. instrumentalness: Float 0.0-1.0 (0 = very vocal, 1 = purely instrumental)

Respond with ONLY a JSON object in this exact format:
{{
  "mood_tags": ["tag1", "tag2", "tag3"],
  "energy_level": 0.7,
  "danceability": 0.6,
  "valence": 0.8,
  "song_type": ["type1", "type2"],
  "themes": ["theme1", "theme2"],
  "acousticness": 0.3,
  "instrumentalness": 0.1
}}"#,
            request.title,
            request.artist,
            request.album,
            request.genres.join(", "),
            request.year.map(|y| y.to_string()).unwrap_or_else(|| "Unknown".to_string())
        );

        let response = self
            .client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &self.anthropic_api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&serde_json::json!({
                "model": "claude-sonnet-4-5-20250929",
                "max_tokens": 1024,
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

        // Strip markdown code fences if present (Claude sometimes wraps JSON in ```json ... ```)
        let json_text = content_text
            .trim()
            .strip_prefix("```json")
            .or_else(|| content_text.trim().strip_prefix("```"))
            .map(|s| s.strip_suffix("```").unwrap_or(s))
            .unwrap_or(content_text)
            .trim();

        // Parse the JSON from the text content
        let analysis: TrackAnalysisResult = serde_json::from_str(json_text)
            .map_err(|e| AppError::ExternalApi(format!("Failed to parse analysis JSON: {}", e)))?;

        Ok(analysis)
    }
}
