//! Audio Pipeline Service
//!
//! Server-side audio processing pipeline that:
//! 1. Fetches audio from Navidrome
//! 2. Decodes to PCM samples (using Symphonia)
//! 3. Manages continuous playback buffer with track transitions
//! 4. Provides samples for encoding/broadcasting

use crate::error::{AppError, Result};
use crate::services::NavidromeClient;
use bytes::Bytes;
use std::collections::VecDeque;
use std::sync::Arc;
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use tokio::sync::{broadcast, mpsc, RwLock};
use tracing::{debug, error, info, warn};

/// Target sample rate for the output stream (CD quality)
pub const OUTPUT_SAMPLE_RATE: u32 = 44100;
/// Number of audio channels (stereo)
pub const OUTPUT_CHANNELS: usize = 2;
/// Samples per second per channel
pub const SAMPLES_PER_SECOND: usize = OUTPUT_SAMPLE_RATE as usize;

/// Configuration for the audio pipeline
#[derive(Debug, Clone)]
pub struct AudioPipelineConfig {
    /// Output sample rate (default: 44100 Hz)
    pub sample_rate: u32,
    /// Buffer size in seconds (how much audio to keep decoded ahead)
    pub buffer_seconds: f32,
    /// Crossfade duration in seconds for track transitions
    pub crossfade_seconds: f32,
    /// Number of audio channels
    pub channels: usize,
}

impl Default for AudioPipelineConfig {
    fn default() -> Self {
        Self {
            sample_rate: OUTPUT_SAMPLE_RATE,
            buffer_seconds: 10.0,
            crossfade_seconds: 3.0,
            channels: OUTPUT_CHANNELS,
        }
    }
}

/// Represents a decoded audio segment
#[derive(Debug, Clone)]
pub struct AudioSegment {
    /// PCM samples (interleaved stereo: L R L R ...)
    pub samples: Vec<f32>,
    /// Sample rate
    pub sample_rate: u32,
    /// Number of channels
    pub channels: usize,
    /// Track ID this segment belongs to
    pub track_id: String,
    /// Position in track (in samples from start)
    pub position: u64,
}

/// State of a track being processed
#[derive(Debug, Clone)]
pub struct TrackState {
    pub track_id: String,
    pub title: String,
    pub artist: String,
    pub duration_secs: f32,
    pub position_secs: f32,
}

/// Events broadcast from the pipeline
#[derive(Debug, Clone)]
pub enum PipelineEvent {
    /// New track started playing
    TrackStarted(TrackState),
    /// Track position update
    PositionUpdate { track_id: String, position_secs: f32 },
    /// Track ended
    TrackEnded { track_id: String },
    /// Pipeline stopped
    Stopped,
    /// Error occurred
    Error(String),
}

/// The audio pipeline that manages continuous audio playback
pub struct AudioPipeline {
    config: AudioPipelineConfig,
    navidrome: Arc<NavidromeClient>,
    /// Ring buffer of decoded PCM samples
    buffer: Arc<RwLock<AudioBuffer>>,
    /// Current playback state
    state: Arc<RwLock<PipelineState>>,
    /// Event broadcaster for pipeline events
    event_tx: broadcast::Sender<PipelineEvent>,
    /// Control channel for pipeline commands
    control_tx: Option<mpsc::Sender<PipelineCommand>>,
}

/// Internal audio buffer
struct AudioBuffer {
    /// Ring buffer of samples (interleaved stereo)
    samples: VecDeque<f32>,
    /// Maximum buffer size in samples (total, all channels)
    max_samples: usize,
    /// Current track being buffered
    current_track: Option<BufferedTrack>,
    /// Next track preloaded for transition
    next_track: Option<BufferedTrack>,
}

struct BufferedTrack {
    track_id: String,
    title: String,
    artist: String,
    /// Total decoded samples for this track
    total_samples: usize,
    /// Samples already consumed
    consumed_samples: usize,
}

struct PipelineState {
    /// Is the pipeline running?
    running: bool,
    /// Current track being played
    current_track: Option<TrackState>,
    /// Queue of tracks to play
    track_queue: VecDeque<QueuedTrack>,
}

#[derive(Debug, Clone)]
pub struct QueuedTrack {
    pub track_id: String,
    pub title: String,
    pub artist: String,
}

enum PipelineCommand {
    QueueTrack(QueuedTrack),
    Skip,
    Stop,
}

impl AudioPipeline {
    /// Create a new audio pipeline
    pub fn new(navidrome: Arc<NavidromeClient>, config: AudioPipelineConfig) -> Self {
        let (event_tx, _) = broadcast::channel(100);
        let max_samples = (config.buffer_seconds * config.sample_rate as f32 * config.channels as f32) as usize;

        Self {
            config,
            navidrome,
            buffer: Arc::new(RwLock::new(AudioBuffer {
                samples: VecDeque::with_capacity(max_samples),
                max_samples,
                current_track: None,
                next_track: None,
            })),
            state: Arc::new(RwLock::new(PipelineState {
                running: false,
                current_track: None,
                track_queue: VecDeque::new(),
            })),
            event_tx,
            control_tx: None,
        }
    }

    /// Subscribe to pipeline events
    pub fn subscribe(&self) -> broadcast::Receiver<PipelineEvent> {
        self.event_tx.subscribe()
    }

    /// Queue a track for playback
    pub async fn queue_track(&self, track: QueuedTrack) -> Result<()> {
        if let Some(tx) = &self.control_tx {
            tx.send(PipelineCommand::QueueTrack(track))
                .await
                .map_err(|e| AppError::InternalMessage(format!("Failed to queue track: {}", e)))?;
        } else {
            // Pipeline not started, queue directly
            let mut state = self.state.write().await;
            state.track_queue.push_back(track);
        }
        Ok(())
    }

    /// Skip to the next track
    pub async fn skip(&self) -> Result<()> {
        if let Some(tx) = &self.control_tx {
            tx.send(PipelineCommand::Skip)
                .await
                .map_err(|e| AppError::InternalMessage(format!("Failed to skip: {}", e)))?;
        }
        Ok(())
    }

    /// Stop the pipeline
    pub async fn stop(&self) -> Result<()> {
        if let Some(tx) = &self.control_tx {
            let _ = tx.send(PipelineCommand::Stop).await;
        }
        let mut state = self.state.write().await;
        state.running = false;
        let _ = self.event_tx.send(PipelineEvent::Stopped);
        Ok(())
    }

    /// Get the current track state
    pub async fn current_track(&self) -> Option<TrackState> {
        self.state.read().await.current_track.clone()
    }

    /// Get the number of tracks in the queue
    pub async fn queue_length(&self) -> usize {
        self.state.read().await.track_queue.len()
    }

    /// Start the pipeline processing loop
    pub async fn start(&mut self) -> Result<()> {
        let (control_tx, mut control_rx) = mpsc::channel::<PipelineCommand>(32);
        self.control_tx = Some(control_tx);

        let navidrome = self.navidrome.clone();
        let buffer = self.buffer.clone();
        let state = self.state.clone();
        let event_tx = self.event_tx.clone();
        let config = self.config.clone();

        {
            let mut s = state.write().await;
            s.running = true;
        }

        // Spawn the main processing task
        tokio::spawn(async move {
            info!("Audio pipeline started");

            // Log initial queue state
            {
                let s = state.read().await;
                info!("Audio pipeline: {} tracks in queue", s.track_queue.len());
                for (i, track) in s.track_queue.iter().enumerate().take(3) {
                    info!("  Queue[{}]: {} - {}", i, track.artist, track.title);
                }
            }

            loop {
                // Check for control commands (non-blocking)
                match control_rx.try_recv() {
                    Ok(PipelineCommand::QueueTrack(track)) => {
                        let mut s = state.write().await;
                        s.track_queue.push_back(track);
                    }
                    Ok(PipelineCommand::Skip) => {
                        // Clear current track, force load next
                        let mut buf = buffer.write().await;
                        buf.samples.clear();
                        buf.current_track = None;
                    }
                    Ok(PipelineCommand::Stop) => {
                        info!("Audio pipeline stopping");
                        let mut s = state.write().await;
                        s.running = false;
                        let _ = event_tx.send(PipelineEvent::Stopped);
                        break;
                    }
                    Err(mpsc::error::TryRecvError::Empty) => {}
                    Err(mpsc::error::TryRecvError::Disconnected) => {
                        warn!("Pipeline control channel disconnected");
                        break;
                    }
                }

                // Check if we need to load more audio
                let needs_audio = {
                    let buf = buffer.read().await;
                    buf.samples.len() < buf.max_samples / 2
                };

                if needs_audio {
                    // Get next track from queue if no current track
                    let (next_track, queue_len) = {
                        let buf = buffer.read().await;
                        let s = state.read().await;
                        let track = if buf.current_track.is_none() {
                            s.track_queue.front().cloned()
                        } else {
                            None
                        };
                        (track, s.track_queue.len())
                    };

                    if next_track.is_none() && queue_len == 0 {
                        // No tracks in queue, wait before checking again
                        debug!("Audio pipeline: waiting for tracks in queue");
                        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
                        continue;
                    }

                    if let Some(track) = next_track {
                        // Pop from queue
                        {
                            let mut s = state.write().await;
                            s.track_queue.pop_front();
                        }

                        info!("Loading track: {} - {} (id: {})", track.artist, track.title, track.track_id);

                        // Fetch and decode track
                        match Self::fetch_and_decode(&navidrome, &track.track_id, &config).await {
                            Ok(samples) => {
                                let duration_secs = samples.len() as f32
                                    / (config.sample_rate as f32 * config.channels as f32);

                                let track_state = TrackState {
                                    track_id: track.track_id.clone(),
                                    title: track.title.clone(),
                                    artist: track.artist.clone(),
                                    duration_secs,
                                    position_secs: 0.0,
                                };

                                {
                                    let mut buf = buffer.write().await;
                                    buf.samples.extend(samples.iter());
                                    buf.current_track = Some(BufferedTrack {
                                        track_id: track.track_id.clone(),
                                        title: track.title.clone(),
                                        artist: track.artist.clone(),
                                        total_samples: samples.len(),
                                        consumed_samples: 0,
                                    });
                                }

                                {
                                    let mut s = state.write().await;
                                    s.current_track = Some(track_state.clone());
                                }

                                let _ = event_tx.send(PipelineEvent::TrackStarted(track_state));
                            }
                            Err(e) => {
                                error!("Failed to load track {}: {}", track.track_id, e);
                                let _ = event_tx.send(PipelineEvent::Error(format!(
                                    "Failed to load {}: {}",
                                    track.title, e
                                )));
                            }
                        }
                    }
                }

                // Small sleep to prevent busy loop
                tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
            }
        });

        Ok(())
    }

    /// Read samples from the buffer (called by broadcaster)
    /// Returns the number of samples actually read
    pub async fn read_samples(&self, output: &mut [f32]) -> usize {
        let mut buffer = self.buffer.write().await;
        let available = buffer.samples.len().min(output.len());

        for (i, sample) in buffer.samples.drain(..available).enumerate() {
            output[i] = sample;
        }

        // Update consumed samples count and position
        if let Some(ref mut track) = buffer.current_track {
            track.consumed_samples += available;

            // Calculate current position in seconds
            let position_secs = track.consumed_samples as f32
                / (OUTPUT_SAMPLE_RATE as f32 * OUTPUT_CHANNELS as f32);

            // Update position in state
            {
                let mut state = self.state.write().await;
                if let Some(ref mut current) = state.current_track {
                    if current.track_id == track.track_id {
                        current.position_secs = position_secs;
                    }
                }
            }

            // Check if track finished
            if track.consumed_samples >= track.total_samples {
                let track_id = track.track_id.clone();
                info!(
                    "Track {} finished: consumed {} of {} samples",
                    track_id, track.consumed_samples, track.total_samples
                );
                buffer.current_track = None;
                let _ = self.event_tx.send(PipelineEvent::TrackEnded { track_id });
            }
        }

        available
    }

    /// Get current buffer fill level (0.0 - 1.0)
    pub async fn buffer_level(&self) -> f32 {
        let buffer = self.buffer.read().await;
        buffer.samples.len() as f32 / buffer.max_samples as f32
    }

    /// Fetch audio from Navidrome and decode to PCM
    async fn fetch_and_decode(
        navidrome: &NavidromeClient,
        track_id: &str,
        config: &AudioPipelineConfig,
    ) -> Result<Vec<f32>> {
        info!("Fetching audio for track {}", track_id);

        // Fetch audio data from Navidrome
        let audio_data = navidrome.stream_track(track_id).await?;

        info!("Received {} bytes from Navidrome, decoding...", audio_data.len());

        // Decode in a blocking task since Symphonia is sync
        let sample_rate = config.sample_rate;
        let channels = config.channels;

        let samples = tokio::task::spawn_blocking(move || {
            Self::decode_audio(&audio_data, sample_rate, channels)
        })
        .await
        .map_err(|e| AppError::InternalMessage(format!("Decode task panicked: {}", e)))??;

        // Calculate track duration from samples
        let duration_secs = samples.len() as f32 / (sample_rate as f32 * channels as f32);
        info!(
            "Decoded {} samples = {:.1} seconds of audio",
            samples.len(),
            duration_secs
        );
        Ok(samples)
    }

    /// Decode audio bytes to PCM samples
    fn decode_audio(data: &[u8], target_sample_rate: u32, target_channels: usize) -> Result<Vec<f32>> {
        let cursor = std::io::Cursor::new(data.to_vec());
        let mss = MediaSourceStream::new(Box::new(cursor), Default::default());

        let probed = symphonia::default::get_probe()
            .format(
                &Hint::new(),
                mss,
                &FormatOptions::default(),
                &MetadataOptions::default(),
            )
            .map_err(|e| AppError::InternalMessage(format!("Failed to probe audio: {}", e)))?;

        let mut format = probed.format;

        // Find the audio track
        let track = format
            .tracks()
            .iter()
            .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
            .ok_or_else(|| AppError::InternalMessage("No audio track found".to_string()))?;

        let track_id = track.id;
        let codec_params = track.codec_params.clone();

        let mut decoder = symphonia::default::get_codecs()
            .make(&codec_params, &DecoderOptions::default())
            .map_err(|e| AppError::InternalMessage(format!("Failed to create decoder: {}", e)))?;

        let mut samples: Vec<f32> = Vec::new();
        let source_sample_rate = codec_params.sample_rate.unwrap_or(44100);
        let source_channels = codec_params.channels.map(|c| c.count()).unwrap_or(2);

        // Decode all packets
        loop {
            let packet = match format.next_packet() {
                Ok(packet) => packet,
                Err(symphonia::core::errors::Error::IoError(e))
                    if e.kind() == std::io::ErrorKind::UnexpectedEof =>
                {
                    break;
                }
                Err(e) => {
                    warn!("Error reading packet: {}", e);
                    break;
                }
            };

            if packet.track_id() != track_id {
                continue;
            }

            let decoded = match decoder.decode(&packet) {
                Ok(decoded) => decoded,
                Err(e) => {
                    warn!("Error decoding packet: {}", e);
                    continue;
                }
            };

            let spec = *decoded.spec();
            let mut sample_buf = SampleBuffer::<f32>::new(decoded.capacity() as u64, spec);
            sample_buf.copy_interleaved_ref(decoded);

            let packet_samples = sample_buf.samples();

            // Handle channel conversion and collect samples
            if source_channels == target_channels {
                samples.extend_from_slice(packet_samples);
            } else if source_channels == 1 && target_channels == 2 {
                // Mono to stereo: duplicate samples
                for &s in packet_samples {
                    samples.push(s);
                    samples.push(s);
                }
            } else if source_channels == 2 && target_channels == 1 {
                // Stereo to mono: average channels
                for chunk in packet_samples.chunks(2) {
                    if chunk.len() == 2 {
                        samples.push((chunk[0] + chunk[1]) / 2.0);
                    }
                }
            } else {
                // Other channel configs: just take first target_channels
                for chunk in packet_samples.chunks(source_channels) {
                    for i in 0..target_channels.min(chunk.len()) {
                        samples.push(chunk[i]);
                    }
                }
            }
        }

        // Resample if needed
        if source_sample_rate != target_sample_rate {
            samples = Self::resample(
                &samples,
                source_sample_rate,
                target_sample_rate,
                target_channels,
            );
        }

        Ok(samples)
    }

    /// Linear interpolation resampling (preserving channel interleaving)
    fn resample(
        samples: &[f32],
        from_rate: u32,
        to_rate: u32,
        channels: usize,
    ) -> Vec<f32> {
        let ratio = from_rate as f64 / to_rate as f64;
        let input_frames = samples.len() / channels;
        let output_frames = (input_frames as f64 / ratio) as usize;
        let mut output = Vec::with_capacity(output_frames * channels);

        for frame in 0..output_frames {
            let src_pos = frame as f64 * ratio;
            let src_frame = src_pos.floor() as usize;
            let next_frame = (src_frame + 1).min(input_frames - 1);
            let frac = (src_pos - src_frame as f64) as f32;

            for ch in 0..channels {
                let curr = samples[src_frame * channels + ch];
                let next = samples[next_frame * channels + ch];
                output.push(curr * (1.0 - frac) + next * frac);
            }
        }

        output
    }

    /// Apply crossfade between two sample buffers
    #[allow(dead_code)]
    fn crossfade(from: &[f32], to: &[f32], fade_samples: usize) -> Vec<f32> {
        let fade_len = fade_samples.min(from.len()).min(to.len());
        let mut result = Vec::with_capacity(from.len() - fade_len + to.len());

        // Copy non-fading part of 'from'
        result.extend_from_slice(&from[..from.len() - fade_len]);

        // Crossfade region
        for i in 0..fade_len {
            let t = i as f32 / fade_len as f32;
            let from_idx = from.len() - fade_len + i;
            let faded = from[from_idx] * (1.0 - t) + to[i] * t;
            result.push(faded);
        }

        // Copy remaining 'to' samples
        result.extend_from_slice(&to[fade_len..]);

        result
    }
}

impl NavidromeClient {
    /// Stream a track and return the raw audio bytes
    pub async fn stream_track(&self, track_id: &str) -> Result<Bytes> {
        let url = format!("{}/rest/stream", self.base_url());

        let params = self.build_params(vec![("id", track_id)]);

        let response = self
            .client()
            .get(&url)
            .query(&params)
            .send()
            .await
            .map_err(|e| AppError::InternalMessage(format!("Failed to stream track: {}", e)))?;

        if !response.status().is_success() {
            return Err(AppError::InternalMessage(format!(
                "Stream request failed: {}",
                response.status()
            )));
        }

        let bytes = response
            .bytes()
            .await
            .map_err(|e| AppError::InternalMessage(format!("Failed to read stream: {}", e)))?;

        Ok(bytes)
    }
}
