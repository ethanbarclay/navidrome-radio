use crate::error::{AppError, Result};
use crate::models::{NowPlaying, Station, Track, TrackInfo};
use crate::services::{CurationEngine, NavidromeClient};
use chrono::{DateTime, Utc, Duration};
use redis::aio::ConnectionManager;
use sqlx::PgPool;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// How long before a listener is considered disconnected (no heartbeat)
const LISTENER_TIMEOUT_SECONDS: i64 = 15;

#[derive(Clone)]
pub struct ActiveStation {
    pub station_id: Uuid,
    pub current_track: Option<Track>,
    pub started_at: Option<DateTime<Utc>>,
    /// Map of session_id -> last heartbeat time
    pub listener_heartbeats: HashMap<String, DateTime<Utc>>,
}

#[derive(Clone)]
pub struct StationManager {
    db: PgPool,
    redis: ConnectionManager,
    active_stations: Arc<RwLock<HashMap<Uuid, ActiveStation>>>,
    curation_engine: Arc<CurationEngine>,
    navidrome_client: Arc<NavidromeClient>,
}

impl StationManager {
    pub fn new(
        db: PgPool,
        redis: ConnectionManager,
        curation_engine: Arc<CurationEngine>,
        navidrome_client: Arc<NavidromeClient>,
    ) -> Self {
        Self {
            db,
            redis,
            active_stations: Arc::new(RwLock::new(HashMap::new())),
            curation_engine,
            navidrome_client,
        }
    }

    pub async fn load_active_stations(&self) -> Result<()> {
        // Load all active stations from database
        let stations: Vec<Station> = sqlx::query_as(
            "SELECT * FROM stations WHERE active = true"
        )
        .fetch_all(&self.db)
        .await?;

        tracing::info!("Loading {} active stations", stations.len());

        for station in stations {
            // Initialize active station
            let mut active_stations = self.active_stations.write().await;
            active_stations.insert(
                station.id,
                ActiveStation {
                    station_id: station.id,
                    current_track: None,
                    started_at: None,
                    listener_heartbeats: HashMap::new(),
                },
            );
            drop(active_stations);

            // Start playing first track
            if let Err(e) = self.play_next_track(station.id).await {
                tracing::error!("Failed to start station {}: {:?}", station.id, e);
            } else {
                tracing::info!("Started station: {} ({})", station.name, station.path);
            }
        }

        Ok(())
    }

    pub async fn start_station(&self, station_id: Uuid) -> Result<()> {
        // Mark station as active in database
        sqlx::query("UPDATE stations SET active = true WHERE id = $1")
            .bind(station_id)
            .execute(&self.db)
            .await?;

        // Initialize active station
        let mut stations = self.active_stations.write().await;
        stations.insert(
            station_id,
            ActiveStation {
                station_id,
                current_track: None,
                started_at: None,
                listener_heartbeats: HashMap::new(),
            },
        );

        // Start playing first track
        drop(stations);
        self.play_next_track(station_id).await?;

        tracing::info!("Started station: {}", station_id);
        Ok(())
    }

    pub async fn stop_station(&self, station_id: Uuid) -> Result<()> {
        // Mark station as inactive in database
        sqlx::query("UPDATE stations SET active = false WHERE id = $1")
            .bind(station_id)
            .execute(&self.db)
            .await?;

        // Remove from active stations
        let mut stations = self.active_stations.write().await;
        stations.remove(&station_id);

        tracing::info!("Stopped station: {}", station_id);
        Ok(())
    }

    pub async fn skip_track(&self, station_id: Uuid) -> Result<()> {
        // Mark current track as skipped in history
        let stations = self.active_stations.read().await;
        if let Some(active) = stations.get(&station_id) {
            if let Some(track) = &active.current_track {
                // Use a subquery to find the most recent entry
                sqlx::query(
                    "UPDATE playlist_history
                     SET skipped = true
                     WHERE id = (
                         SELECT id FROM playlist_history
                         WHERE station_id = $1 AND track_id = $2
                         ORDER BY played_at DESC
                         LIMIT 1
                     )",
                )
                .bind(station_id)
                .bind(&track.id)
                .execute(&self.db)
                .await?;
            }
        }
        drop(stations);

        // Play next track
        self.play_next_track(station_id).await?;

        Ok(())
    }

    pub async fn play_next_track(&self, station_id: Uuid) -> Result<()> {
        // Get station
        let station = self.get_station_by_id(station_id).await?;

        // Get recent tracks to avoid repetition
        let recent_tracks = self.get_recent_tracks(station_id, 20).await?;
        let recent_ids: Vec<String> = recent_tracks.iter().map(|t| t.clone()).collect();

        // Select next track
        let track = self
            .curation_engine
            .select_next_track(&station, &recent_ids)
            .await?;

        let now = Utc::now();

        // Save to playlist history
        sqlx::query(
            "INSERT INTO playlist_history (station_id, track_id, played_at, selection_method)
             VALUES ($1, $2, $3, $4)",
        )
        .bind(station_id)
        .bind(&track.id)
        .bind(now)
        .bind("random")
        .execute(&self.db)
        .await?;

        // Update active station
        let mut stations = self.active_stations.write().await;
        if let Some(active) = stations.get_mut(&station_id) {
            active.current_track = Some(track.clone());
            active.started_at = Some(now);
        }

        tracing::info!("Playing track '{}' on station {}", track.title, station_id);

        Ok(())
    }

    pub async fn get_now_playing(&self, station_id: Uuid) -> Result<NowPlaying> {
        // Check if current track has ended
        let should_advance = {
            let stations = self.active_stations.read().await;
            if let Some(active) = stations.get(&station_id) {
                if let (Some(track), Some(started_at)) = (&active.current_track, active.started_at) {
                    let elapsed = (Utc::now() - started_at).num_seconds();
                    // If track has ended, we need to advance
                    elapsed >= track.duration as i64
                } else {
                    false
                }
            } else {
                false
            }
        };

        // Advance if needed (outside the lock)
        if should_advance {
            tracing::info!("Track ended for station {}, advancing to next", station_id);
            self.play_next_track(station_id).await?;
        }

        // Get current now playing after potential track advance
        let stations = self.active_stations.read().await;
        let active = stations
            .get(&station_id)
            .ok_or_else(|| AppError::NotFound("Station not active".to_string()))?;

        let track = active
            .current_track
            .clone()
            .ok_or_else(|| AppError::NotFound("No track playing".to_string()))?;

        // Count active listeners (heartbeat within timeout)
        let now = Utc::now();
        let timeout = Duration::seconds(LISTENER_TIMEOUT_SECONDS);
        let active_listeners = active
            .listener_heartbeats
            .values()
            .filter(|&last_heartbeat| now - *last_heartbeat < timeout)
            .count();

        Ok(NowPlaying {
            track: track.into(),
            started_at: active.started_at.unwrap_or_else(Utc::now),
            listeners: active_listeners,
        })
    }

    /// Record a heartbeat for a listener session. Returns the current listener count.
    pub async fn listener_heartbeat(&self, station_id: Uuid, session_id: String) -> Result<usize> {
        let now = Utc::now();
        let timeout = Duration::seconds(LISTENER_TIMEOUT_SECONDS);

        let mut stations = self.active_stations.write().await;
        if let Some(active) = stations.get_mut(&station_id) {
            // Update this session's heartbeat
            active.listener_heartbeats.insert(session_id, now);

            // Clean up stale sessions while we're here
            active.listener_heartbeats.retain(|_, last_heartbeat| {
                now - *last_heartbeat < timeout
            });

            Ok(active.listener_heartbeats.len())
        } else {
            Err(AppError::NotFound("Station not active".to_string()))
        }
    }

    /// Remove a listener session
    pub async fn listener_leave(&self, station_id: Uuid, session_id: &str) -> Result<()> {
        let mut stations = self.active_stations.write().await;
        if let Some(active) = stations.get_mut(&station_id) {
            active.listener_heartbeats.remove(session_id);
        }
        Ok(())
    }

    /// Get the current listener count for a station
    pub async fn get_listener_count(&self, station_id: Uuid) -> Result<usize> {
        let now = Utc::now();
        let timeout = Duration::seconds(LISTENER_TIMEOUT_SECONDS);

        let stations = self.active_stations.read().await;
        if let Some(active) = stations.get(&station_id) {
            let count = active
                .listener_heartbeats
                .values()
                .filter(|&last_heartbeat| now - *last_heartbeat < timeout)
                .count();
            Ok(count)
        } else {
            Ok(0)
        }
    }

    /// Get listener counts for all active stations
    pub async fn get_all_listener_counts(&self) -> HashMap<Uuid, usize> {
        let now = Utc::now();
        let timeout = Duration::seconds(LISTENER_TIMEOUT_SECONDS);

        let stations = self.active_stations.read().await;
        stations
            .iter()
            .map(|(id, active)| {
                let count = active
                    .listener_heartbeats
                    .values()
                    .filter(|&last_heartbeat| now - *last_heartbeat < timeout)
                    .count();
                (*id, count)
            })
            .collect()
    }

    async fn get_station_by_id(&self, station_id: Uuid) -> Result<Station> {
        sqlx::query_as::<_, Station>("SELECT * FROM stations WHERE id = $1")
            .bind(station_id)
            .fetch_optional(&self.db)
            .await?
            .ok_or_else(|| AppError::NotFound("Station not found".to_string()))
    }

    async fn get_recent_tracks(&self, station_id: Uuid, limit: i64) -> Result<Vec<String>> {
        let tracks: Vec<(String,)> = sqlx::query_as(
            "SELECT track_id FROM playlist_history
             WHERE station_id = $1
             ORDER BY played_at DESC
             LIMIT $2",
        )
        .bind(station_id)
        .bind(limit)
        .fetch_all(&self.db)
        .await?;

        Ok(tracks.into_iter().map(|(id,)| id).collect())
    }

    pub fn get_stream_url(&self, track_id: &str) -> String {
        // For MVP, we'll proxy directly to Navidrome
        format!("/api/stream/{}", track_id)
    }
}
