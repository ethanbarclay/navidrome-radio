//! Audio Broadcaster Service
//!
//! Encodes PCM audio from the pipeline and broadcasts via HLS (HTTP Live Streaming).
//! Creates MP3 segments and generates m3u8 playlists for clients.

use crate::error::{AppError, Result};
use crate::services::audio_pipeline::{AudioPipeline, PipelineEvent, OUTPUT_CHANNELS, OUTPUT_SAMPLE_RATE};
use mp3lame_encoder::{Builder, FlushNoGap, InterleavedPcm};
use rustfft::{num_complex::Complex, FftPlanner};
use std::collections::VecDeque;
use std::mem::MaybeUninit;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use tracing::{debug, error, info, warn};

/// HLS segment duration in seconds
pub const HLS_SEGMENT_DURATION: f32 = 2.0;
/// Number of segments to keep in the sliding window playlist
pub const HLS_PLAYLIST_LENGTH: usize = 5;
/// Number of FFT bins for visualization
pub const FFT_SIZE: usize = 2048;
/// Visualization update rate (Hz)
pub const VIZ_UPDATE_RATE: u32 = 30;

/// Configuration for the audio broadcaster
#[derive(Debug, Clone)]
pub struct AudioBroadcasterConfig {
    /// HLS segment duration in seconds
    pub segment_duration: f32,
    /// Number of segments to keep in playlist
    pub playlist_length: usize,
    /// MP3 bitrate in kbps
    pub bitrate: u32,
    /// Enable visualization data generation
    pub enable_visualization: bool,
}

impl Default for AudioBroadcasterConfig {
    fn default() -> Self {
        Self {
            segment_duration: HLS_SEGMENT_DURATION,
            playlist_length: HLS_PLAYLIST_LENGTH,
            bitrate: 192,
            enable_visualization: true,
        }
    }
}

/// An HLS audio segment
#[derive(Debug, Clone)]
pub struct HlsSegment {
    /// Segment sequence number
    pub sequence: u64,
    /// Duration in seconds
    pub duration: f32,
    /// MP3 encoded audio data
    pub data: Vec<u8>,
    /// Track ID for this segment
    pub track_id: String,
}

/// Visualization data for a time slice
#[derive(Debug, Clone, serde::Serialize)]
pub struct VisualizationData {
    /// Timestamp (milliseconds since broadcast start)
    pub timestamp_ms: u64,
    /// Frequency spectrum (normalized 0-1, typically 64 bins)
    pub spectrum: Vec<f32>,
    /// Current RMS level (0-1)
    pub level: f32,
    /// Beat detected in this frame
    pub beat: bool,
    /// Current track ID
    pub track_id: String,
}

/// Broadcaster state shared across requests
pub struct BroadcasterState {
    /// Circular buffer of recent segments
    segments: VecDeque<HlsSegment>,
    /// Current segment sequence number
    sequence: u64,
    /// Target playlist length
    playlist_length: usize,
    /// Current track info
    current_track_id: String,
    /// Media sequence of first segment in playlist
    media_sequence: u64,
    /// Whether a discontinuity occurred (e.g., track skip)
    discontinuity: bool,
}

/// The audio broadcaster that encodes and serves HLS streams
pub struct AudioBroadcaster {
    config: AudioBroadcasterConfig,
    pipeline: Arc<AudioPipeline>,
    state: Arc<RwLock<BroadcasterState>>,
    /// Broadcast channel for visualization data
    viz_tx: broadcast::Sender<VisualizationData>,
    /// Running flag
    running: Arc<std::sync::atomic::AtomicBool>,
    /// Broadcast start time for timestamps
    start_time: Arc<AtomicU64>,
    /// Signal to clear local buffers (set by skip, cleared by broadcast loop)
    clear_buffers: Arc<std::sync::atomic::AtomicBool>,
}

impl AudioBroadcaster {
    /// Create a new audio broadcaster
    pub fn new(pipeline: Arc<AudioPipeline>, config: AudioBroadcasterConfig) -> Self {
        let (viz_tx, _) = broadcast::channel(100);

        Self {
            config: config.clone(),
            pipeline,
            state: Arc::new(RwLock::new(BroadcasterState {
                segments: VecDeque::with_capacity(config.playlist_length + 2),
                sequence: 0,
                playlist_length: config.playlist_length,
                current_track_id: String::new(),
                media_sequence: 0,
                discontinuity: false,
            })),
            viz_tx,
            running: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            start_time: Arc::new(AtomicU64::new(0)),
            clear_buffers: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }

    /// Subscribe to visualization data updates
    pub fn subscribe_visualization(&self) -> broadcast::Receiver<VisualizationData> {
        self.viz_tx.subscribe()
    }

    /// Check if broadcaster is running
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }

    /// Get the current track being played by the pipeline
    pub async fn current_track(&self) -> Option<crate::services::audio_pipeline::TrackState> {
        self.pipeline.current_track().await
    }

    /// Skip to the next track in the pipeline
    pub async fn skip(&self) -> crate::error::Result<()> {
        // Signal the broadcast loop to clear its local buffers
        self.clear_buffers.store(true, Ordering::SeqCst);

        // Skip in the pipeline (clears pipeline's internal buffer)
        self.pipeline.skip().await?;

        // Clear all buffered segments and mark discontinuity
        {
            let mut state = self.state.write().await;
            let old_count = state.segments.len();
            state.media_sequence += old_count as u64;
            state.segments.clear();
            state.discontinuity = true;
            info!("Skip: cleared {} segments, set clear_buffers flag, marked discontinuity", old_count);
        }

        Ok(())
    }

    /// Start the broadcaster
    pub async fn start(&self) -> Result<()> {
        if self.running.swap(true, Ordering::SeqCst) {
            return Ok(()); // Already running
        }

        let start = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        self.start_time.store(start, Ordering::Relaxed);

        let pipeline = self.pipeline.clone();
        let state = self.state.clone();
        let viz_tx = self.viz_tx.clone();
        let config = self.config.clone();
        let running = self.running.clone();
        let start_time = self.start_time.clone();
        let clear_buffers = self.clear_buffers.clone();

        // Subscribe to pipeline events for track changes
        let mut pipeline_events = pipeline.subscribe();

        // Spawn the encoding loop
        tokio::spawn(async move {
            info!("Audio broadcaster started");

            // Samples needed per segment
            let samples_per_segment = (config.segment_duration * OUTPUT_SAMPLE_RATE as f32) as usize
                * OUTPUT_CHANNELS;

            // Buffer for accumulating samples
            let mut sample_buffer: Vec<f32> = Vec::with_capacity(samples_per_segment);

            // FFT setup for visualization
            let mut fft_planner = FftPlanner::new();
            let fft = fft_planner.plan_fft_forward(FFT_SIZE);
            let samples_per_viz = OUTPUT_SAMPLE_RATE as usize / VIZ_UPDATE_RATE as usize * OUTPUT_CHANNELS;
            let mut viz_buffer: Vec<f32> = Vec::with_capacity(samples_per_viz);

            // Beat detection state
            let mut energy_history: VecDeque<f32> = VecDeque::with_capacity(43); // ~1.4s at 30Hz
            let mut last_beat_time: u64 = 0;

            let mut current_track = String::new();

            // Real-time throttling: track when we started and how many segments we've produced
            let broadcast_start = std::time::Instant::now();
            let segment_duration_ms = (config.segment_duration * 1000.0) as u64;
            // Allow producing up to 3 segments ahead of real-time for buffering
            let max_lead_segments: u64 = 3;

            // Read loop
            let mut read_buffer = vec![0.0f32; 4096];

            while running.load(Ordering::Relaxed) {
                // Check if skip was requested - clear local buffers
                if clear_buffers.swap(false, Ordering::SeqCst) {
                    info!("Broadcaster: clearing local buffers due to skip");
                    sample_buffer.clear();
                    viz_buffer.clear();
                    energy_history.clear();
                }

                // Check for track changes
                match pipeline_events.try_recv() {
                    Ok(PipelineEvent::TrackStarted(track)) => {
                        current_track = track.track_id.clone();
                        let mut st = state.write().await;
                        st.current_track_id = track.track_id;
                        info!("Broadcaster: track started - {} - {}", track.artist, track.title);
                    }
                    Ok(PipelineEvent::TrackEnded { .. }) => {
                        debug!("Broadcaster: track ended");
                    }
                    Ok(PipelineEvent::Stopped) => {
                        info!("Broadcaster: pipeline stopped");
                        break;
                    }
                    Ok(PipelineEvent::Error(e)) => {
                        error!("Broadcaster: pipeline error: {}", e);
                    }
                    _ => {}
                }

                // Read samples from pipeline
                let samples_read = pipeline.read_samples(&mut read_buffer).await;

                if samples_read == 0 {
                    // No samples available, wait a bit
                    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
                    continue;
                }

                // Add to segment buffer
                sample_buffer.extend_from_slice(&read_buffer[..samples_read]);

                // Add to visualization buffer
                viz_buffer.extend_from_slice(&read_buffer[..samples_read]);

                // Process visualization if we have enough samples
                while viz_buffer.len() >= samples_per_viz {
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_millis() as u64;
                    let timestamp = now - start_time.load(Ordering::Relaxed);

                    // Compute spectrum and level
                    let (spectrum, level) = Self::compute_visualization(
                        &viz_buffer[..samples_per_viz],
                        &fft,
                    );

                    // Simple beat detection based on energy spike
                    let beat = Self::detect_beat(
                        level,
                        &mut energy_history,
                        &mut last_beat_time,
                        timestamp,
                    );

                    let viz_data = VisualizationData {
                        timestamp_ms: timestamp,
                        spectrum,
                        level,
                        beat,
                        track_id: current_track.clone(),
                    };

                    // Send visualization (ignore if no subscribers)
                    let _ = viz_tx.send(viz_data);

                    // Remove processed samples
                    viz_buffer.drain(..samples_per_viz);
                }

                // Create segment when buffer is full
                if sample_buffer.len() >= samples_per_segment {
                    // Real-time throttling: check if we're too far ahead
                    let current_sequence = {
                        let st = state.read().await;
                        st.sequence
                    };

                    // Calculate when this segment SHOULD be produced in real-time
                    // Segment N represents audio from time N*segment_duration to (N+1)*segment_duration
                    let expected_time_ms = current_sequence * segment_duration_ms;
                    let actual_elapsed_ms = broadcast_start.elapsed().as_millis() as u64;
                    let max_lead_ms = max_lead_segments * segment_duration_ms;

                    // If we're more than max_lead_segments ahead, wait
                    if actual_elapsed_ms + max_lead_ms < expected_time_ms {
                        let wait_ms = expected_time_ms - actual_elapsed_ms - max_lead_ms;
                        debug!(
                            "Throttling: segment {} would be {:.1}s ahead, waiting {}ms",
                            current_sequence,
                            (expected_time_ms - actual_elapsed_ms) as f32 / 1000.0,
                            wait_ms
                        );
                        tokio::time::sleep(tokio::time::Duration::from_millis(wait_ms)).await;
                    }

                    let segment_samples: Vec<f32> = sample_buffer.drain(..samples_per_segment).collect();

                    // Encode to MP3 - each segment is independently decodable
                    let mp3_data = tokio::task::spawn_blocking(move || {
                        Self::encode_segment(&segment_samples)
                    })
                    .await
                    .unwrap_or_else(|e| {
                        error!("Encoding task panicked: {}", e);
                        Vec::new()
                    });

                    // Skip empty segments
                    if mp3_data.is_empty() {
                        warn!("Segment encoding produced no data, skipping");
                        continue;
                    }

                    let mut st = state.write().await;
                    let sequence = st.sequence;
                    st.sequence += 1;

                    let segment = HlsSegment {
                        sequence,
                        duration: config.segment_duration,
                        data: mp3_data,
                        track_id: st.current_track_id.clone(),
                    };

                    // Add to circular buffer
                    st.segments.push_back(segment);

                    // Remove old segments
                    while st.segments.len() > st.playlist_length + 2 {
                        st.segments.pop_front();
                        st.media_sequence += 1;
                    }

                    info!(
                        "Created segment {} ({} bytes, {:.1}s into broadcast)",
                        sequence,
                        st.segments.back().map(|s| s.data.len()).unwrap_or(0),
                        broadcast_start.elapsed().as_secs_f32()
                    );
                }
            }

            info!("Audio broadcaster stopped");
        });

        Ok(())
    }

    /// Stop the broadcaster
    pub fn stop(&self) {
        self.running.store(false, Ordering::Relaxed);
    }

    /// Generate the HLS playlist (m3u8)
    pub async fn get_playlist(&self) -> String {
        let mut state = self.state.write().await;

        let mut playlist = String::new();
        playlist.push_str("#EXTM3U\n");
        playlist.push_str("#EXT-X-VERSION:3\n");
        playlist.push_str(&format!(
            "#EXT-X-TARGETDURATION:{}\n",
            self.config.segment_duration.ceil() as u32
        ));
        playlist.push_str(&format!("#EXT-X-MEDIA-SEQUENCE:{}\n", state.media_sequence));

        // Only include segments that actually exist
        if state.segments.is_empty() {
            debug!("HLS playlist: no segments available yet");
        }

        // Add discontinuity tag if a skip occurred
        let has_discontinuity = state.discontinuity;
        if has_discontinuity {
            state.discontinuity = false; // Clear the flag
        }

        for (i, segment) in state.segments.iter().enumerate() {
            // Add discontinuity before the first segment after a skip
            if i == 0 && has_discontinuity {
                playlist.push_str("#EXT-X-DISCONTINUITY\n");
            }
            playlist.push_str(&format!("#EXTINF:{:.3},\n", segment.duration));
            playlist.push_str(&format!("segment/{}.mp3\n", segment.sequence));
        }

        debug!(
            "HLS playlist: {} segments, sequence range {}-{}, discontinuity: {}",
            state.segments.len(),
            state.segments.front().map(|s| s.sequence).unwrap_or(0),
            state.segments.back().map(|s| s.sequence).unwrap_or(0),
            has_discontinuity
        );

        playlist
    }

    /// Get a specific segment by sequence number
    pub async fn get_segment(&self, sequence: u64) -> Option<HlsSegment> {
        let state = self.state.read().await;
        state
            .segments
            .iter()
            .find(|s| s.sequence == sequence)
            .cloned()
    }

    /// Get the number of segments currently available
    pub async fn segment_count(&self) -> usize {
        let state = self.state.read().await;
        state.segments.len()
    }

    /// Get the current stream URL for clients
    pub fn get_stream_url(&self, station_id: &str) -> String {
        format!("/api/v1/stations/{}/stream/playlist.m3u8", station_id)
    }

    /// Encode PCM samples to MP3 format - creates a complete, independently decodable MP3 segment
    fn encode_segment(samples: &[f32]) -> Vec<u8> {
        // Create a fresh encoder for each segment to ensure complete frames
        let mut builder = match Builder::new() {
            Some(b) => b,
            None => {
                error!("Failed to create MP3 encoder builder");
                return Vec::new();
            }
        };

        if let Err(e) = builder.set_num_channels(OUTPUT_CHANNELS as u8) {
            error!("Failed to set channels: {:?}", e);
            return Vec::new();
        }
        if let Err(e) = builder.set_sample_rate(OUTPUT_SAMPLE_RATE) {
            error!("Failed to set sample rate: {:?}", e);
            return Vec::new();
        }
        if let Err(e) = builder.set_brate(mp3lame_encoder::Birtate::Kbps192) {
            error!("Failed to set bitrate: {:?}", e);
            return Vec::new();
        }
        if let Err(e) = builder.set_quality(mp3lame_encoder::Quality::Best) {
            error!("Failed to set quality: {:?}", e);
            return Vec::new();
        }

        let mut encoder = match builder.build() {
            Ok(enc) => enc,
            Err(e) => {
                error!("Failed to build encoder: {:?}", e);
                return Vec::new();
            }
        };

        // Convert f32 samples to i16
        let pcm: Vec<i16> = samples
            .iter()
            .map(|&s| (s.clamp(-1.0, 1.0) * 32767.0) as i16)
            .collect();

        // Allocate output buffer
        let mp3_buffer_size = (pcm.len() as f32 * 1.25) as usize + 7200;
        let mut mp3_buffer: Vec<MaybeUninit<u8>> = vec![MaybeUninit::uninit(); mp3_buffer_size];

        // Encode to MP3
        let input = InterleavedPcm(&pcm);
        let bytes_written = match encoder.encode(input, &mut mp3_buffer) {
            Ok(size) => size,
            Err(e) => {
                error!("MP3 encoding failed: {:?}", e);
                return Vec::new();
            }
        };

        // Flush to complete the MP3 frames
        let flush_buffer_size = 7200;
        let mut flush_buffer: Vec<MaybeUninit<u8>> = vec![MaybeUninit::uninit(); flush_buffer_size];
        let flush_written = match encoder.flush::<FlushNoGap>(&mut flush_buffer) {
            Ok(size) => size,
            Err(e) => {
                error!("MP3 flush failed: {:?}", e);
                0
            }
        };

        // Combine encoded data and flush data
        let total_size = bytes_written + flush_written;
        let mut mp3_data = Vec::with_capacity(total_size);
        unsafe {
            mp3_data.extend_from_slice(
                std::slice::from_raw_parts(mp3_buffer.as_ptr() as *const u8, bytes_written)
            );
            if flush_written > 0 {
                mp3_data.extend_from_slice(
                    std::slice::from_raw_parts(flush_buffer.as_ptr() as *const u8, flush_written)
                );
            }
        }

        debug!(
            "Encoded segment: {} samples -> {} bytes MP3",
            samples.len(),
            mp3_data.len()
        );

        mp3_data
    }

    /// Compute visualization data from samples
    fn compute_visualization(
        samples: &[f32],
        fft: &Arc<dyn rustfft::Fft<f32>>,
    ) -> (Vec<f32>, f32) {
        // Convert stereo to mono for analysis
        let mono: Vec<f32> = samples
            .chunks(OUTPUT_CHANNELS)
            .map(|chunk| chunk.iter().sum::<f32>() / OUTPUT_CHANNELS as f32)
            .collect();

        // Compute RMS level
        let rms: f32 = (mono.iter().map(|&s| s * s).sum::<f32>() / mono.len() as f32).sqrt();

        // Prepare FFT input (with Hann window)
        let fft_len = FFT_SIZE.min(mono.len());
        let mut fft_input: Vec<Complex<f32>> = mono[..fft_len]
            .iter()
            .enumerate()
            .map(|(i, &s)| {
                let window = 0.5 * (1.0 - (2.0 * std::f32::consts::PI * i as f32 / fft_len as f32).cos());
                Complex::new(s * window, 0.0)
            })
            .collect();

        // Pad to FFT_SIZE if needed
        fft_input.resize(FFT_SIZE, Complex::new(0.0, 0.0));

        // Run FFT
        fft.process(&mut fft_input);

        // Compute magnitude spectrum (first half only)
        let magnitudes: Vec<f32> = fft_input[..FFT_SIZE / 2]
            .iter()
            .map(|c| c.norm() / FFT_SIZE as f32)
            .collect();

        // Bin down to ~64 bars for visualization
        let num_bars = 64;
        let bins_per_bar = magnitudes.len() / num_bars;
        let spectrum: Vec<f32> = (0..num_bars)
            .map(|i| {
                let start = i * bins_per_bar;
                let end = start + bins_per_bar;
                let avg: f32 = magnitudes[start..end].iter().sum::<f32>() / bins_per_bar as f32;
                // Log scale and normalize
                (1.0 + avg).ln() / 5.0
            })
            .collect();

        (spectrum, rms)
    }

    /// Simple beat detection based on energy spikes
    fn detect_beat(
        level: f32,
        history: &mut VecDeque<f32>,
        last_beat: &mut u64,
        current_time: u64,
    ) -> bool {
        history.push_back(level);
        if history.len() > 43 {
            history.pop_front();
        }

        if history.len() < 10 {
            return false;
        }

        // Calculate average energy
        let avg: f32 = history.iter().sum::<f32>() / history.len() as f32;

        // Calculate variance
        let variance: f32 = history.iter().map(|&e| (e - avg).powi(2)).sum::<f32>() / history.len() as f32;

        // Beat threshold: current level significantly above average
        let threshold = avg + 1.5 * variance.sqrt();

        // Minimum time between beats (150ms ~= 400 BPM max)
        let min_beat_interval = 150;

        if level > threshold && level > 0.1 && (current_time - *last_beat) > min_beat_interval {
            *last_beat = current_time;
            true
        } else {
            false
        }
    }
}
