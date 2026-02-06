//! Hybrid Curator
//!
//! Combines LLM-based seed selection with audio encoder similarity search
//! to create high-quality, sonically coherent playlists.
//!
//! Flow:
//! 1. LLM selects 5-10 "perfect" seed songs based on query
//! 2. Seeds are placed evenly throughout the playlist
//! 3. Audio encoder fills gaps with sonically similar tracks
//! 4. Result: Playlist that matches query AND flows smoothly

#![allow(dead_code)]

use crate::error::{AppError, Result};
use crate::services::audio_encoder::AudioEncoder;
use crate::services::seed_selector::{SeedSelector, VerifiedSeed};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

/// Progress updates for hybrid curation
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "step", rename_all = "snake_case")]
pub enum HybridCurationProgress {
    Started {
        query: String,
        message: String,
    },
    CheckingEmbeddings {
        message: String,
        coverage_percent: f32,
    },
    SelectingSeeds {
        message: String,
    },
    SeedsSelected {
        message: String,
        count: usize,
        seeds: Vec<String>, // "Artist - Title" format
    },
    GeneratingEmbeddings {
        message: String,
        current: usize,
        total: usize,
        track_name: String,
    },
    FillingGaps {
        message: String,
        segment: usize,
        total_segments: usize,
        from_seed: String,
        to_seed: String,
    },
    Completed {
        message: String,
        total_tracks: usize,
        seed_count: usize,
        filled_count: usize,
        method: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        track_ids: Option<Vec<String>>,
    },
    Error {
        message: String,
    },
}

/// Configuration for hybrid curation
#[derive(Debug, Clone)]
pub struct HybridCurationConfig {
    /// Number of seed songs to select
    pub seed_count: usize,
    /// Total playlist size
    pub playlist_size: usize,
    /// Minimum embedding coverage required (0.0 to 1.0)
    pub min_embedding_coverage: f32,
    /// Fall back to traditional curation if embedding coverage is low
    pub fallback_enabled: bool,
}

impl Default for HybridCurationConfig {
    fn default() -> Self {
        Self {
            seed_count: 5,
            playlist_size: 50,
            min_embedding_coverage: 0.03, // TODO: Temporarily lowered for testing, restore to 0.3
            fallback_enabled: true,
        }
    }
}

/// Hybrid curator combining LLM seeds with audio similarity
pub struct HybridCurator {
    seed_selector: SeedSelector,
    audio_encoder: Option<Arc<AudioEncoder>>,
    db: PgPool,
    config: HybridCurationConfig,
    library_path: Option<std::path::PathBuf>,
}

impl HybridCurator {
    pub fn new(
        anthropic_api_key: String,
        audio_encoder: Option<Arc<AudioEncoder>>,
        db: PgPool,
        config: HybridCurationConfig,
        library_path: Option<std::path::PathBuf>,
    ) -> Self {
        Self {
            seed_selector: SeedSelector::new(anthropic_api_key, db.clone()),
            audio_encoder,
            db,
            config,
            library_path,
        }
    }

    /// Curate a playlist using hybrid approach
    pub async fn curate(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<String>> {
        let (tx, _rx) = mpsc::channel(10);
        self.curate_with_progress(query, limit, tx).await
    }

    /// Curate with progress updates
    pub async fn curate_with_progress(
        &self,
        query: &str,
        limit: usize,
        progress_tx: mpsc::Sender<HybridCurationProgress>,
    ) -> Result<Vec<String>> {
        let send = |p: HybridCurationProgress| {
            let tx = progress_tx.clone();
            async move { let _ = tx.send(p).await; }
        };

        send(HybridCurationProgress::Started {
            query: query.to_string(),
            message: "Starting hybrid curation...".to_string(),
        }).await;

        // Check embedding coverage
        let coverage = self.get_embedding_coverage().await?;
        info!("Embedding coverage: {:.1}%", coverage * 100.0);

        send(HybridCurationProgress::CheckingEmbeddings {
            message: format!("Audio embedding coverage: {:.1}%", coverage * 100.0),
            coverage_percent: coverage * 100.0,
        }).await;

        // Decide on approach based on coverage
        let use_hybrid = coverage >= self.config.min_embedding_coverage;

        if !use_hybrid {
            if self.config.fallback_enabled {
                warn!(
                    "Low embedding coverage ({:.1}%), falling back to LLM-only curation",
                    coverage * 100.0
                );
                let playlist = self.fallback_curation(query, limit, &progress_tx).await?;
                return Ok(playlist);
            } else {
                warn!("Low embedding coverage but fallback disabled, proceeding anyway");
            }
        }

        // Step 1: Select seed songs
        send(HybridCurationProgress::SelectingSeeds {
            message: "AI is selecting perfect seed songs...".to_string(),
        }).await;

        let seeds = self
            .seed_selector
            .select_seeds(query, self.config.seed_count, limit)
            .await?;

        if seeds.is_empty() {
            warn!("No seeds selected, falling back to traditional curation");
            let playlist = self.fallback_curation(query, limit, &progress_tx).await?;
            return Ok(playlist);
        }

        send(HybridCurationProgress::SeedsSelected {
            message: format!("Selected {} seed tracks", seeds.len()),
            count: seeds.len(),
            seeds: seeds
                .iter()
                .map(|s| format!("{} - {}", s.artist, s.title))
                .collect(),
        }).await;

        // Step 2: Fill gaps between seeds using audio similarity
        let playlist = self
            .fill_gaps_between_seeds(&seeds, limit, &progress_tx)
            .await?;

        send(HybridCurationProgress::Completed {
            message: format!("Created playlist with {} tracks", playlist.len()),
            total_tracks: playlist.len(),
            seed_count: seeds.len(),
            filled_count: playlist.len() - seeds.len(),
            method: "hybrid".to_string(),
            track_ids: Some(playlist.clone()),
        }).await;

        Ok(playlist)
    }

    /// Fill gaps between seed songs using audio similarity
    ///
    /// Uses centroid-based similarity (average similarity to ALL seeds) rather than
    /// per-seed similarity. This ensures selected tracks fit the overall vibe,
    /// not just happen to match one seed coincidentally.
    async fn fill_gaps_between_seeds(
        &self,
        seeds: &[VerifiedSeed],
        total_size: usize,
        progress_tx: &mpsc::Sender<HybridCurationProgress>,
    ) -> Result<Vec<String>> {
        let audio_encoder = self.audio_encoder.as_ref().ok_or_else(|| {
            AppError::InternalMessage("Audio encoder not available".to_string())
        })?;

        // Check which seeds are missing embeddings and generate them
        let seeds_needing_embeddings = self.check_missing_embeddings(seeds).await?;

        if !seeds_needing_embeddings.is_empty() {
            info!(
                "{} seed tracks need embedding generation",
                seeds_needing_embeddings.len()
            );

            // Generate embeddings for missing seeds
            self.generate_missing_embeddings(&seeds_needing_embeddings, progress_tx)
                .await?;
        }

        // Collect all seed IDs for centroid-based similarity
        let seed_ids: Vec<String> = seeds.iter().map(|s| s.track_id.clone()).collect();

        // Calculate how many tracks we need to fill
        let tracks_to_fill = total_size.saturating_sub(seeds.len());

        let _ = progress_tx
            .send(HybridCurationProgress::FillingGaps {
                message: format!("Finding {} tracks similar to all {} seeds (using centroid)", tracks_to_fill, seeds.len()),
                segment: 1,
                total_segments: 1,
                from_seed: format!("Centroid of {} seeds", seeds.len()),
                to_seed: "".to_string(),
            })
            .await;

        // Find tracks with highest AVERAGE similarity to all seeds using centroid
        // This is more discriminative than max similarity to any single seed
        let similar_tracks = match audio_encoder
            .find_similar_to_seeds(&seed_ids, tracks_to_fill, &[])
            .await
        {
            Ok(tracks) => tracks,
            Err(e) => {
                warn!("Failed to find tracks similar to seed centroid: {}", e);
                Vec::new()
            }
        };

        info!(
            "Found {} tracks similar to seed centroid (requested {})",
            similar_tracks.len(),
            tracks_to_fill
        );

        // Build playlist by interleaving seeds with similar tracks
        let mut playlist = Vec::with_capacity(total_size);
        let mut similar_iter = similar_tracks.into_iter();

        // Calculate tracks per gap for even distribution
        let num_gaps = seeds.len();
        let tracks_per_gap = tracks_to_fill / num_gaps;
        let remainder = tracks_to_fill % num_gaps;

        for (i, seed) in seeds.iter().enumerate() {
            // Add seed
            playlist.push(seed.track_id.clone());

            // Calculate gap size (distribute remainder among first gaps)
            let gap_size = if i < remainder {
                tracks_per_gap + 1
            } else {
                tracks_per_gap
            };

            // Fill gap with similar tracks
            for _ in 0..gap_size {
                if let Some((track_id, _similarity)) = similar_iter.next() {
                    playlist.push(track_id);
                }
            }
        }

        debug!(
            "Built playlist with {} tracks ({} seeds, {} filled using centroid similarity)",
            playlist.len(),
            seeds.len(),
            playlist.len() - seeds.len()
        );

        Ok(playlist)
    }

    /// Check which seeds are missing embeddings
    async fn check_missing_embeddings(&self, seeds: &[VerifiedSeed]) -> Result<Vec<VerifiedSeed>> {
        let seed_ids: Vec<String> = seeds.iter().map(|s| s.track_id.clone()).collect();

        // Query for seeds that have embeddings
        let tracks_with_embeddings: Vec<String> = sqlx::query_scalar(
            r#"
            SELECT track_id
            FROM track_embeddings
            WHERE track_id = ANY($1)
            "#,
        )
        .bind(&seed_ids)
        .fetch_all(&self.db)
        .await?;

        // Find seeds that don't have embeddings
        let missing: Vec<VerifiedSeed> = seeds
            .iter()
            .filter(|s| !tracks_with_embeddings.contains(&s.track_id))
            .cloned()
            .collect();

        Ok(missing)
    }

    /// Generate embeddings for seeds that are missing them
    async fn generate_missing_embeddings(
        &self,
        seeds: &[VerifiedSeed],
        progress_tx: &mpsc::Sender<HybridCurationProgress>,
    ) -> Result<()> {
        let audio_encoder = self.audio_encoder.as_ref().ok_or_else(|| {
            AppError::InternalMessage("Audio encoder not available".to_string())
        })?;

        let library_path = self.library_path.as_ref().ok_or_else(|| {
            AppError::InternalMessage(
                "Library path not configured - cannot generate embeddings".to_string(),
            )
        })?;

        let total = seeds.len();
        for (i, seed) in seeds.iter().enumerate() {
            let track_name = format!("{} - {}", seed.artist, seed.title);

            let _ = progress_tx
                .send(HybridCurationProgress::GeneratingEmbeddings {
                    message: format!("Generating audio embedding ({}/{})", i + 1, total),
                    current: i + 1,
                    total,
                    track_name: track_name.clone(),
                })
                .await;

            // Get the track's file path from the database
            let path_result: Option<String> = sqlx::query_scalar(
                "SELECT path FROM library_index WHERE id = $1",
            )
            .bind(&seed.track_id)
            .fetch_optional(&self.db)
            .await?;

            if let Some(relative_path) = path_result {
                let full_path = library_path.join(&relative_path);

                if full_path.exists() {
                    match audio_encoder.process_track(&seed.track_id, &full_path).await {
                        Ok(_) => {
                            info!("Generated embedding for seed: {}", track_name);
                        }
                        Err(e) => {
                            warn!("Failed to generate embedding for {}: {}", track_name, e);
                        }
                    }
                } else {
                    warn!("Track file not found for {}: {:?}", track_name, full_path);
                }
            } else {
                warn!("No path found in database for track: {}", seed.track_id);
            }
        }

        Ok(())
    }

    /// Get current embedding coverage (percentage of library with embeddings)
    async fn get_embedding_coverage(&self) -> Result<f32> {
        let result = sqlx::query!(
            r#"
            SELECT
                (SELECT COUNT(*) FROM track_embeddings) as "with_embeddings!",
                (SELECT COUNT(*) FROM library_index) as "total!"
            "#
        )
        .fetch_one(&self.db)
        .await?;

        if result.total == 0 {
            return Ok(0.0);
        }

        Ok(result.with_embeddings as f32 / result.total as f32)
    }

    /// Fallback to simple LLM-based curation when embeddings aren't available
    async fn fallback_curation(
        &self,
        query: &str,
        limit: usize,
        progress_tx: &mpsc::Sender<HybridCurationProgress>,
    ) -> Result<Vec<String>> {
        warn!("Using fallback curation (low embedding coverage)");

        let _ = progress_tx
            .send(HybridCurationProgress::SelectingSeeds {
                message: "Using LLM-only curation (low embedding coverage)...".to_string(),
            })
            .await;

        // Just select seeds and pad with random tracks from same genres
        let seeds = self
            .seed_selector
            .select_seeds(query, self.config.seed_count.min(limit), limit)
            .await?;

        if seeds.is_empty() {
            // Ultimate fallback: random tracks
            let playlist = self.get_random_tracks(limit).await?;
            let _ = progress_tx
                .send(HybridCurationProgress::Completed {
                    message: format!("Selected {} random tracks", playlist.len()),
                    total_tracks: playlist.len(),
                    seed_count: 0,
                    filled_count: playlist.len(),
                    method: "random".to_string(),
                    track_ids: Some(playlist.clone()),
                })
                .await;
            return Ok(playlist);
        }

        let _ = progress_tx
            .send(HybridCurationProgress::SeedsSelected {
                message: format!("AI selected {} tracks", seeds.len()),
                count: seeds.len(),
                seeds: seeds
                    .iter()
                    .map(|s| format!("{} - {}", s.artist, s.title))
                    .collect(),
            })
            .await;

        let mut playlist: Vec<String> = seeds.iter().map(|s| s.track_id.clone()).collect();

        // Fill with similar genre tracks
        let seed_ids: Vec<String> = playlist.clone();
        let remaining = limit.saturating_sub(playlist.len());

        if remaining > 0 {
            let similar = sqlx::query_scalar!(
                r#"
                SELECT li.id
                FROM library_index li
                WHERE li.id != ALL($1)
                AND EXISTS (
                    SELECT 1 FROM library_index seed
                    WHERE seed.id = ANY($1)
                    AND seed.genres ?| (SELECT array_agg(g) FROM jsonb_array_elements_text(li.genres) g)
                )
                LIMIT $2
                "#,
                &seed_ids,
                remaining as i64
            )
            .fetch_all(&self.db)
            .await?;

            playlist.extend(similar);
        }

        let _ = progress_tx
            .send(HybridCurationProgress::Completed {
                message: format!("Created playlist with {} tracks", playlist.len()),
                total_tracks: playlist.len(),
                seed_count: seeds.len(),
                filled_count: playlist.len() - seeds.len(),
                method: "llm".to_string(),
                track_ids: Some(playlist.clone()),
            })
            .await;

        Ok(playlist)
    }

    /// Get random tracks as ultimate fallback
    async fn get_random_tracks(&self, limit: usize) -> Result<Vec<String>> {
        let tracks = sqlx::query_scalar!(
            "SELECT id FROM library_index ORDER BY RANDOM() LIMIT $1",
            limit as i64
        )
        .fetch_all(&self.db)
        .await?;

        Ok(tracks)
    }

    /// Extend an existing playlist with more tracks
    ///
    /// Uses the last few tracks' embeddings to find similar music
    pub async fn extend_playlist(
        &self,
        current_track_ids: &[String],
        count: usize,
    ) -> Result<Vec<String>> {
        let audio_encoder = self.audio_encoder.as_ref().ok_or_else(|| {
            AppError::InternalMessage("Audio encoder not available for extension".to_string())
        })?;

        if current_track_ids.is_empty() {
            return Err(AppError::BadRequest(
                "Cannot extend empty playlist".to_string(),
            ));
        }

        // Use last 3 tracks to determine direction
        let context_size = 3.min(current_track_ids.len());
        let context_tracks = &current_track_ids[current_track_ids.len() - context_size..];

        let mut new_tracks = Vec::new();
        let mut exclude: Vec<String> = current_track_ids.to_vec();

        // Find similar to each context track, then interleave
        for track_id in context_tracks {
            let similar = audio_encoder
                .find_similar(track_id, count / context_size + 1, &exclude)
                .await?;

            for (id, _) in similar {
                if new_tracks.len() >= count {
                    break;
                }
                exclude.push(id.clone());
                new_tracks.push(id);
            }
        }

        Ok(new_tracks)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gap_calculation() {
        // 50 tracks, 5 seeds = 45 to fill = 9 per gap
        let total = 50;
        let seeds = 5;
        let tracks_per_gap = (total - seeds) / seeds;
        assert_eq!(tracks_per_gap, 9);

        // 50 tracks, 7 seeds = 43 to fill = 6 per gap with 1 remainder
        let total = 50;
        let seeds = 7;
        let tracks_per_gap = (total - seeds) / seeds;
        let remainder = (total - seeds) % seeds;
        assert_eq!(tracks_per_gap, 6);
        assert_eq!(remainder, 1);
    }
}
