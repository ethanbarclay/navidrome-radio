//! Audio Encoder Service
//!
//! Generates 100-dimensional embeddings from audio files using the Deej-AI audio encoder.
//! These embeddings capture musical similarity based on actual audio features.
//!
//! Model: teticio/audio-encoder (trained on 1M+ Spotify playlists)
//! Input: Mel spectrogram (5 second windows)
//! Output: 100-dimensional embedding vector

use crate::error::{AppError, Result};
use ndarray::{Array2, Array4, Axis};
use ort::execution_providers::CoreMLExecutionProvider;
use ort::session::{builder::GraphOptimizationLevel, Session};
use rustfft::{num_complex::Complex, FftPlanner};
use sqlx::PgPool;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::DecoderOptions;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use tokio::sync::Semaphore;
use tracing::{debug, info, warn};

/// Audio encoder configuration
pub struct AudioEncoderConfig {
    /// Path to ONNX model file
    pub model_path: PathBuf,
    /// Sample rate for audio processing (model expects 22050 Hz)
    pub sample_rate: u32,
    /// Number of mel filterbanks
    pub n_mels: usize,
    /// FFT window size
    pub n_fft: usize,
    /// Hop length between frames
    pub hop_length: usize,
    /// Duration of audio to process (in seconds)
    pub duration_secs: f32,
    /// Maximum concurrent encoding operations
    pub max_concurrent: usize,
}

impl Default for AudioEncoderConfig {
    fn default() -> Self {
        // Use all available cores (M1 has 8)
        let num_cores = std::thread::available_parallelism()
            .map(|p| p.get())
            .unwrap_or(8);

        // Parameters matching teticio/audio-encoder model:
        // - n_mels = 96
        // - slice_size = 216 frames (model expects exactly 216 frames)
        // - 22050 Hz sample rate
        // - With hop_length=512 and 216 frames, we need 216*512 = 110,592 samples
        // - At 22050 Hz, that's about 5 seconds of audio
        Self {
            model_path: PathBuf::from("models/audio_encoder.onnx"),
            sample_rate: 22050,
            n_mels: 96,  // Model expects 96 mel bins
            n_fft: 2048,
            hop_length: 512,
            duration_secs: 5.0,
            max_concurrent: num_cores,
        }
    }
}

/// A pool of ONNX sessions for parallel inference
struct SessionPool {
    sessions: Vec<tokio::sync::Mutex<Session>>,
    next_idx: std::sync::atomic::AtomicUsize,
}

impl SessionPool {
    fn new(sessions: Vec<Session>) -> Self {
        Self {
            sessions: sessions.into_iter().map(tokio::sync::Mutex::new).collect(),
            next_idx: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    /// Get the next session using round-robin selection
    async fn get(&self) -> tokio::sync::MutexGuard<'_, Session> {
        let idx = self.next_idx.fetch_add(1, std::sync::atomic::Ordering::Relaxed) % self.sessions.len();
        self.sessions[idx].lock().await
    }
}

/// Audio encoder for generating music embeddings
pub struct AudioEncoder {
    session_pool: Arc<SessionPool>,
    config: AudioEncoderConfig,
    db: PgPool,
    semaphore: Semaphore,
}

impl AudioEncoder {
    /// Create a new audio encoder with the given configuration
    pub fn new(config: AudioEncoderConfig, db: PgPool) -> Result<Self> {
        info!(
            "Loading audio encoder model from {:?} with {} parallel sessions",
            config.model_path, config.max_concurrent
        );

        // Create a pool of sessions for true parallel inference
        // Each session can run inference independently
        let pool_size = config.max_concurrent.min(4); // 4 sessions is usually enough
        let mut sessions = Vec::with_capacity(pool_size);

        // Detect number of cores for optimal thread allocation
        let num_cores = std::thread::available_parallelism()
            .map(|p| p.get())
            .unwrap_or(8);
        let threads_per_session = (num_cores / pool_size).max(1);

        for i in 0..pool_size {
            info!(
                "Creating ONNX session {}/{} with {} threads",
                i + 1,
                pool_size,
                threads_per_session
            );

            // Try to use CoreML on macOS for Apple Neural Engine acceleration
            let coreml = CoreMLExecutionProvider::default()
                .with_subgraphs(true) // Enable CoreML on subgraphs
                .build();

            let session = Session::builder()
                .map_err(|e| AppError::InternalMessage(format!("Failed to create session builder: {}", e)))?
                .with_execution_providers([coreml])
                .map_err(|e| {
                    warn!("CoreML not available, falling back to CPU: {}", e);
                    AppError::InternalMessage(format!("Failed to set execution provider: {}", e))
                })
                .unwrap_or_else(|_| {
                    // Fallback: create session without CoreML
                    Session::builder().unwrap()
                })
                .with_optimization_level(GraphOptimizationLevel::Level3)
                .map_err(|e| AppError::InternalMessage(format!("Failed to set optimization level: {}", e)))?
                .with_intra_threads(threads_per_session)
                .map_err(|e| AppError::InternalMessage(format!("Failed to set threads: {}", e)))?
                .commit_from_file(&config.model_path)
                .map_err(|e| AppError::InternalMessage(format!("Failed to load ONNX model: {}", e)))?;

            sessions.push(session);
        }

        let max_concurrent = config.max_concurrent;

        Ok(Self {
            session_pool: Arc::new(SessionPool::new(sessions)),
            config,
            db,
            semaphore: Semaphore::new(max_concurrent),
        })
    }

    /// Encode an audio file and return its 100-dimensional embedding
    pub async fn encode_file(&self, audio_path: &Path) -> Result<Vec<f32>> {
        let _permit = self.semaphore.acquire().await.map_err(|e| {
            AppError::InternalMessage(format!("Failed to acquire semaphore: {}", e))
        })?;

        let path = audio_path.to_path_buf();
        let config = AudioEncoderConfig {
            model_path: self.config.model_path.clone(),
            sample_rate: self.config.sample_rate,
            n_mels: self.config.n_mels,
            n_fft: self.config.n_fft,
            hop_length: self.config.hop_length,
            duration_secs: self.config.duration_secs,
            max_concurrent: self.config.max_concurrent,
        };

        // Pre-process audio (CPU-bound but doesn't need session)
        let mel_spec = tokio::task::spawn_blocking(move || {
            Self::load_and_preprocess(&path, &config)
        })
        .await
        .map_err(|e| AppError::InternalMessage(format!("Preprocessing task panicked: {}", e)))??;

        // Acquire a session from the pool and run inference
        let mut session = self.session_pool.get().await;
        Self::run_inference_async(&mut session, mel_spec)
    }

    /// Load audio and compute mel spectrogram (CPU-bound preprocessing)
    fn load_and_preprocess(audio_path: &Path, config: &AudioEncoderConfig) -> Result<Array4<f32>> {
        debug!("Loading and preprocessing audio file: {:?}", audio_path);

        // Load and decode audio
        let samples = Self::load_audio(audio_path, config.sample_rate)?;

        // Generate mel spectrogram
        Self::compute_mel_spectrogram(
            &samples,
            config.sample_rate,
            config.n_fft,
            config.hop_length,
            config.n_mels,
        )
    }

    /// Run inference with an async-compatible session guard
    fn run_inference_async(
        session: &mut tokio::sync::MutexGuard<'_, Session>,
        mel_spec: Array4<f32>,
    ) -> Result<Vec<f32>> {
        use ort::value::Tensor;

        // Create input tensor
        let input_tensor = Tensor::from_array(mel_spec)
            .map_err(|e| AppError::InternalMessage(format!("Failed to create input tensor: {}", e)))?;

        // Run inference
        let outputs = session
            .run(ort::inputs![input_tensor])
            .map_err(|e| AppError::InternalMessage(format!("ONNX inference failed: {}", e)))?;

        // Extract output tensor
        let (_, output) = outputs
            .into_iter()
            .next()
            .ok_or_else(|| AppError::InternalMessage("No output from model".to_string()))?;

        let (_, embedding_data) = output
            .try_extract_tensor::<f32>()
            .map_err(|e| AppError::InternalMessage(format!("Failed to extract embedding: {}", e)))?;

        let embedding: Vec<f32> = embedding_data.iter().cloned().collect();

        // Debug: log embedding stats
        let emb_min = embedding.iter().cloned().fold(f32::INFINITY, f32::min);
        let emb_max = embedding.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
        let emb_mean: f32 = embedding.iter().sum::<f32>() / embedding.len() as f32;
        let emb_norm: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
        debug!(
            "Embedding stats: len={}, min={:.4}, max={:.4}, mean={:.4}, norm={:.4}",
            embedding.len(), emb_min, emb_max, emb_mean, emb_norm
        );
        debug!("First 5 embedding values: {:?}", &embedding[..5.min(embedding.len())]);

        Ok(embedding)
    }

    /// Load and decode audio file to mono float samples
    fn load_audio(path: &Path, target_sample_rate: u32) -> Result<Vec<f32>> {
        let file = std::fs::File::open(path)
            .map_err(|e| AppError::InternalMessage(format!("Failed to open audio file: {}", e)))?;

        let mss = MediaSourceStream::new(Box::new(file), Default::default());

        let mut hint = Hint::new();
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            hint.with_extension(ext);
        }

        let probed = symphonia::default::get_probe()
            .format(&hint, mss, &FormatOptions::default(), &MetadataOptions::default())
            .map_err(|e| AppError::InternalMessage(format!("Failed to probe audio format: {}", e)))?;

        let mut format = probed.format;
        let track = format
            .default_track()
            .ok_or_else(|| AppError::InternalMessage("No audio track found".to_string()))?;

        let mut decoder = symphonia::default::get_codecs()
            .make(&track.codec_params, &DecoderOptions::default())
            .map_err(|e| AppError::InternalMessage(format!("Failed to create decoder: {}", e)))?;

        let track_id = track.id;
        let mut samples = Vec::new();

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

            let channel_samples = sample_buf.samples();
            let n_channels = spec.channels.count();

            // Convert to mono by averaging channels
            for chunk in channel_samples.chunks(n_channels) {
                let mono: f32 = chunk.iter().sum::<f32>() / n_channels as f32;
                samples.push(mono);
            }
        }

        // Resample if necessary (simple linear interpolation)
        // TODO: Use a proper resampling library for better quality
        let original_rate = decoder.codec_params().sample_rate.unwrap_or(44100);
        if original_rate != target_sample_rate {
            samples = Self::resample(&samples, original_rate, target_sample_rate);
        }

        Ok(samples)
    }

    /// Simple linear interpolation resampling
    fn resample(samples: &[f32], from_rate: u32, to_rate: u32) -> Vec<f32> {
        let ratio = from_rate as f64 / to_rate as f64;
        let output_len = (samples.len() as f64 / ratio) as usize;
        let mut output = Vec::with_capacity(output_len);

        for i in 0..output_len {
            let src_idx = i as f64 * ratio;
            let idx_floor = src_idx.floor() as usize;
            let idx_ceil = (idx_floor + 1).min(samples.len() - 1);
            let frac = src_idx - idx_floor as f64;

            let value = samples[idx_floor] * (1.0 - frac as f32)
                + samples[idx_ceil] * frac as f32;
            output.push(value);
        }

        output
    }

    /// Compute mel spectrogram from audio samples
    ///
    /// Matches the preprocessing from teticio/audio-encoder (audiodiffusion):
    /// 1. Compute mel spectrogram using librosa-compatible parameters
    /// 2. Convert to dB scale (power_to_db with ref=max, top_db=80)
    /// 3. Normalize to 0-1 range
    /// 4. Resize to exactly 216 frames (required by model)
    fn compute_mel_spectrogram(
        samples: &[f32],
        sample_rate: u32,
        n_fft: usize,
        hop_length: usize,
        n_mels: usize,
    ) -> Result<Array4<f32>> {
        // The model expects exactly 216 frames (slice_size in Deej-AI)
        const TARGET_FRAMES: usize = 216;

        let mut planner = FftPlanner::new();
        let fft = planner.plan_fft_forward(n_fft);

        // Create mel filterbank
        let mel_filterbank = Self::create_mel_filterbank(n_mels, n_fft, sample_rate as f32);

        // Calculate number of frames from available samples
        let n_frames = (samples.len().saturating_sub(n_fft)) / hop_length + 1;
        if n_frames == 0 {
            return Err(AppError::InternalMessage("Audio too short for analysis".to_string()));
        }

        // Compute STFT and mel spectrogram (power values)
        let mut mel_spec = Array2::zeros((n_mels, n_frames));
        let window = Self::hann_window(n_fft);

        for (frame_idx, start) in (0..samples.len().saturating_sub(n_fft))
            .step_by(hop_length)
            .enumerate()
        {
            if frame_idx >= n_frames {
                break;
            }

            // Apply window and convert to complex
            let mut buffer: Vec<Complex<f32>> = samples[start..start + n_fft]
                .iter()
                .zip(window.iter())
                .map(|(&s, &w)| Complex::new(s * w, 0.0))
                .collect();

            // Compute FFT
            fft.process(&mut buffer);

            // Compute power spectrum (first half + 1)
            let power_spec: Vec<f32> = buffer[..n_fft / 2 + 1]
                .iter()
                .map(|c| c.norm_sqr())
                .collect();

            // Apply mel filterbank (keep as power, not log yet)
            for (mel_idx, mel_filter) in mel_filterbank.iter().enumerate() {
                let mel_energy: f32 = power_spec
                    .iter()
                    .zip(mel_filter.iter())
                    .map(|(&p, &f)| p * f)
                    .sum();
                mel_spec[[mel_idx, frame_idx]] = mel_energy;
            }
        }

        // Convert to dB scale (like librosa.power_to_db with ref=np.max, top_db=80)
        // The audiodiffusion library uses top_db=80 for normalization
        let top_db: f32 = 80.0;
        let max_power = mel_spec.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
        let ref_power = max_power.max(1e-10); // Avoid log of zero

        for val in mel_spec.iter_mut() {
            // power_to_db formula: 10 * log10(S / ref)
            // With ref=max, this gives 0 for the loudest and negative for quieter
            let db_val = 10.0 * (*val / ref_power).max(1e-10).log10();
            // Clip to top_db range: values below -top_db become -top_db
            *val = db_val.max(-top_db);
        }

        // Normalize to 0-1 range using fixed top_db (like audiodiffusion preprocessing)
        // Formula: (log_S + top_db) / top_db
        // This maps [-80, 0] dB to [0, 1]
        for val in mel_spec.iter_mut() {
            *val = (*val + top_db) / top_db;
        }

        // Debug: log mel spectrogram stats
        let mel_min = mel_spec.iter().cloned().fold(f32::INFINITY, f32::min);
        let mel_max = mel_spec.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
        let mel_mean: f32 = mel_spec.iter().sum::<f32>() / mel_spec.len() as f32;
        debug!(
            "Mel spectrogram stats: shape=({}, {}), min={:.4}, max={:.4}, mean={:.4}",
            mel_spec.shape()[0], mel_spec.shape()[1], mel_min, mel_max, mel_mean
        );

        // Resize to exactly TARGET_FRAMES (216) frames using linear interpolation
        // The model has a fixed input size and expects exactly 216 frames
        let resized = Self::resize_spectrogram(&mel_spec, n_mels, TARGET_FRAMES);

        // Debug: log resized mel stats
        let res_min = resized.iter().cloned().fold(f32::INFINITY, f32::min);
        let res_max = resized.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
        let res_mean: f32 = resized.iter().sum::<f32>() / resized.len() as f32;
        debug!(
            "Resized mel stats: shape=({}, {}), min={:.4}, max={:.4}, mean={:.4}",
            resized.shape()[0], resized.shape()[1], res_min, res_max, res_mean
        );

        // Reshape to (1, 1, n_mels, TARGET_FRAMES) for model input (batch, channels, n_mels, n_frames)
        Ok(resized.insert_axis(Axis(0)).insert_axis(Axis(0)))
    }

    /// Resize spectrogram to target number of frames using linear interpolation
    fn resize_spectrogram(mel_spec: &Array2<f32>, n_mels: usize, target_frames: usize) -> Array2<f32> {
        let current_frames = mel_spec.shape()[1];

        if current_frames == target_frames {
            return mel_spec.clone();
        }

        let mut resized = Array2::zeros((n_mels, target_frames));
        let scale = current_frames as f32 / target_frames as f32;

        for mel in 0..n_mels {
            for target_frame in 0..target_frames {
                // Map target frame to source frame
                let src_pos = target_frame as f32 * scale;
                let src_floor = src_pos.floor() as usize;
                let src_ceil = (src_floor + 1).min(current_frames - 1);
                let frac = src_pos - src_floor as f32;

                // Linear interpolation
                let value = mel_spec[[mel, src_floor]] * (1.0 - frac)
                    + mel_spec[[mel, src_ceil]] * frac;
                resized[[mel, target_frame]] = value;
            }
        }

        resized
    }

    /// Create Hann window
    fn hann_window(size: usize) -> Vec<f32> {
        (0..size)
            .map(|i| {
                0.5 * (1.0 - (2.0 * std::f32::consts::PI * i as f32 / (size - 1) as f32).cos())
            })
            .collect()
    }

    /// Create mel filterbank matrix with slaney normalization (like librosa default)
    fn create_mel_filterbank(n_mels: usize, n_fft: usize, sample_rate: f32) -> Vec<Vec<f32>> {
        let n_bins = n_fft / 2 + 1;

        // Mel scale conversion functions (HTK formula)
        let hz_to_mel = |hz: f32| 2595.0 * (1.0 + hz / 700.0).log10();
        let mel_to_hz = |mel: f32| 700.0 * (10.0_f32.powf(mel / 2595.0) - 1.0);

        let mel_min = hz_to_mel(0.0);
        let mel_max = hz_to_mel(sample_rate / 2.0);

        // Create mel points
        let mel_points: Vec<f32> = (0..n_mels + 2)
            .map(|i| mel_min + (mel_max - mel_min) * i as f32 / (n_mels + 1) as f32)
            .collect();

        // Convert mel points to Hz
        let hz_points: Vec<f32> = mel_points.iter().map(|&m| mel_to_hz(m)).collect();

        // Convert to FFT bin indices (using floor like librosa)
        let bin_points: Vec<usize> = hz_points
            .iter()
            .map(|&hz| ((n_fft as f32 + 1.0) * hz / sample_rate).floor() as usize)
            .collect();

        // Create filterbank
        let mut filterbank = vec![vec![0.0f32; n_bins]; n_mels];

        for i in 0..n_mels {
            let start = bin_points[i];
            let center = bin_points[i + 1];
            let end = bin_points[i + 2];

            // Slaney normalization: normalize by bandwidth in Hz
            // This ensures each filter has unit area, so energy is comparable across frequencies
            let bandwidth_hz = hz_points[i + 2] - hz_points[i];
            let norm_factor = 2.0 / bandwidth_hz;

            // Rising edge
            for j in start..center {
                if j < n_bins && center > start {
                    filterbank[i][j] = norm_factor * (j - start) as f32 / (center - start) as f32;
                }
            }

            // Falling edge
            for j in center..end {
                if j < n_bins && end > center {
                    filterbank[i][j] = norm_factor * (end - j) as f32 / (end - center) as f32;
                }
            }
        }

        filterbank
    }

    /// Normalize a vector to unit length (L2 norm = 1)
    /// This allows using L2 distance for better similarity spread
    fn normalize_embedding(embedding: Vec<f32>) -> Vec<f32> {
        let norm: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 1e-10 {
            embedding.into_iter().map(|x| x / norm).collect()
        } else {
            embedding
        }
    }

    /// Process a track and store its embedding in the database
    pub async fn process_track(&self, track_id: &str, audio_path: &Path) -> Result<()> {
        let start = Instant::now();

        // Check if already processed
        let exists = sqlx::query_scalar!(
            "SELECT 1 FROM track_embeddings WHERE track_id = $1",
            track_id
        )
        .fetch_optional(&self.db)
        .await?;

        if exists.is_some() {
            debug!("Track {} already has embedding, skipping", track_id);
            return Ok(());
        }

        // Encode the audio
        match self.encode_file(audio_path).await {
            Ok(embedding) => {
                let processing_time = start.elapsed().as_millis() as i32;

                // Normalize embedding to unit length for L2 distance similarity
                let normalized = Self::normalize_embedding(embedding);

                // Format vector as string for safe SQL binding (avoids binary protocol issues)
                let vec_str = format!(
                    "[{}]",
                    normalized
                        .iter()
                        .map(|v| v.to_string())
                        .collect::<Vec<_>>()
                        .join(",")
                );

                // Store embedding using raw SQL with string cast
                sqlx::query(
                    r#"
                    INSERT INTO track_embeddings (track_id, embedding, processing_time_ms)
                    VALUES ($1, $2::vector, $3)
                    ON CONFLICT (track_id) DO UPDATE SET
                        embedding = EXCLUDED.embedding,
                        computed_at = NOW(),
                        processing_time_ms = EXCLUDED.processing_time_ms
                    "#,
                )
                .bind(track_id)
                .bind(&vec_str)
                .bind(processing_time)
                .execute(&self.db)
                .await?;

                info!(
                    "Stored embedding for track {} ({} ms)",
                    track_id, processing_time
                );
                Ok(())
            }
            Err(e) => {
                // Record failure for retry later
                sqlx::query!(
                    r#"
                    INSERT INTO embedding_failures (track_id, error_message, error_type)
                    VALUES ($1, $2, $3)
                    ON CONFLICT (track_id) DO UPDATE SET
                        error_message = EXCLUDED.error_message,
                        attempt_count = embedding_failures.attempt_count + 1,
                        last_attempt = NOW()
                    "#,
                    track_id,
                    e.to_string(),
                    "encode_error"
                )
                .execute(&self.db)
                .await?;

                Err(e)
            }
        }
    }

    /// Find tracks similar to a given track
    /// Filters by genre to ensure results are in compatible genres with the source track
    pub async fn find_similar(
        &self,
        track_id: &str,
        limit: usize,
        exclude_ids: &[String],
    ) -> Result<Vec<(String, f32)>> {
        // Get the source track's embedding first
        let source_embedding = self.get_embedding(track_id).await?;
        let source_embedding = source_embedding.ok_or_else(|| {
            AppError::InternalMessage(format!("No embedding for track {}", track_id))
        })?;

        // Convert to pgvector::Vector and format as string for safe SQL binding
        let vec_str = format!(
            "[{}]",
            source_embedding
                .iter()
                .map(|v| v.to_string())
                .collect::<Vec<_>>()
                .join(",")
        );

        // Use raw SQL with L2 distance (<->) for better similarity spread
        // For normalized vectors, L2 distance ranges [0, 2], convert to similarity [1, 0]
        // Also filter by genre to ensure results share at least one genre with the source
        let results = sqlx::query_as::<_, (String, f64)>(
            r#"
            WITH source_genres AS (
                SELECT DISTINCT g.genre
                FROM library_index li,
                     jsonb_array_elements_text(li.genres) AS g(genre)
                WHERE li.id = $2
            ),
            allowed_genres AS (
                SELECT array_agg(genre) as genres FROM source_genres
            )
            SELECT
                te.track_id,
                1.0 - (te.embedding <-> $1::vector) / 2.0 as similarity
            FROM track_embeddings te
            JOIN library_index li ON te.track_id = li.id
            CROSS JOIN allowed_genres ag
            WHERE te.track_id != $2
            AND te.track_id != ALL($3)
            AND (ag.genres IS NULL OR li.genres ?| ag.genres)
            ORDER BY te.embedding <-> $1::vector
            LIMIT $4
            "#,
        )
        .bind(&vec_str)
        .bind(track_id)
        .bind(exclude_ids)
        .bind(limit as i64)
        .fetch_all(&self.db)
        .await?;

        Ok(results
            .into_iter()
            .map(|(id, sim)| (id, sim as f32))
            .collect())
    }

    /// Find transition tracks between two songs
    /// Filters by genre to ensure results share at least one genre with the source tracks
    pub async fn find_transition_tracks(
        &self,
        from_track_id: &str,
        to_track_id: &str,
        count: usize,
        exclude_ids: &[String],
    ) -> Result<Vec<String>> {
        // For proper interpolation, we need to do this in application code
        // since SQL doesn't support vector arithmetic easily

        // Get embeddings for both tracks
        let from_emb = self.get_embedding(from_track_id).await?;
        let to_emb = self.get_embedding(to_track_id).await?;

        let from_emb = from_emb.ok_or_else(|| {
            AppError::InternalMessage(format!("No embedding for track {}", from_track_id))
        })?;
        let to_emb = to_emb.ok_or_else(|| {
            AppError::InternalMessage(format!("No embedding for track {}", to_track_id))
        })?;

        // Find tracks at interpolation points
        let mut result = Vec::new();
        let mut all_exclude: Vec<String> = exclude_ids.to_vec();
        all_exclude.push(from_track_id.to_string());
        all_exclude.push(to_track_id.to_string());

        // Collect both source track IDs for genre filtering
        let source_ids = vec![from_track_id.to_string(), to_track_id.to_string()];

        for i in 1..=count {
            let t = i as f32 / (count + 1) as f32;
            let interp: Vec<f32> = from_emb
                .iter()
                .zip(to_emb.iter())
                .map(|(&a, &b)| a * (1.0 - t) + b * t)
                .collect();

            // Normalize the interpolated vector and format for SQL binding
            let interp_normed = Self::normalize_embedding(interp);
            let vec_str = format!(
                "[{}]",
                interp_normed
                    .iter()
                    .map(|v| v.to_string())
                    .collect::<Vec<_>>()
                    .join(",")
            );

            // Find closest track to interpolation point using L2 distance
            // Also filter by genre to ensure results share genres with source tracks
            let closest: Option<String> = sqlx::query_scalar(
                r#"
                WITH source_genres AS (
                    SELECT DISTINCT g.genre
                    FROM library_index li,
                         jsonb_array_elements_text(li.genres) AS g(genre)
                    WHERE li.id = ANY($3)
                ),
                allowed_genres AS (
                    SELECT array_agg(genre) as genres FROM source_genres
                )
                SELECT te.track_id
                FROM track_embeddings te
                JOIN library_index li ON te.track_id = li.id
                CROSS JOIN allowed_genres ag
                WHERE te.track_id != ALL($2)
                AND (ag.genres IS NULL OR li.genres ?| ag.genres)
                ORDER BY te.embedding <-> $1::vector
                LIMIT 1
                "#,
            )
            .bind(&vec_str)
            .bind(&all_exclude)
            .bind(&source_ids)
            .fetch_optional(&self.db)
            .await?;

            if let Some(track_id) = closest {
                all_exclude.push(track_id.clone());
                result.push(track_id);
            }
        }

        Ok(result)
    }

    /// Get embedding for a track
    async fn get_embedding(&self, track_id: &str) -> Result<Option<Vec<f32>>> {
        // Use raw SQL to avoid binary protocol issues with pgvector
        // The embedding::text cast converts to "[0.1,0.2,...]" format
        let result: Option<String> = sqlx::query_scalar(
            r#"SELECT embedding::text FROM track_embeddings WHERE track_id = $1"#,
        )
        .bind(track_id)
        .fetch_optional(&self.db)
        .await?;

        // Parse the text representation back to Vec<f32>
        Ok(result.map(|text| {
            // Text format is "[0.1,0.2,0.3,...]"
            text.trim_start_matches('[')
                .trim_end_matches(']')
                .split(',')
                .filter_map(|s| s.trim().parse::<f32>().ok())
                .collect()
        }))
    }

    /// Find tracks with highest average similarity to multiple seed tracks
    /// This is better than max similarity because it ensures tracks fit the overall vibe,
    /// not just happen to match one seed coincidentally.
    ///
    /// Also filters by genre to ensure tracks are in a compatible genre with the seeds.
    pub async fn find_similar_to_seeds(
        &self,
        seed_ids: &[String],
        limit: usize,
        exclude_ids: &[String],
    ) -> Result<Vec<(String, f32)>> {
        if seed_ids.is_empty() {
            return Ok(Vec::new());
        }

        // Get embeddings for all seeds
        let mut seed_embeddings: Vec<Vec<f32>> = Vec::new();
        for seed_id in seed_ids {
            if let Some(emb) = self.get_embedding(seed_id).await? {
                seed_embeddings.push(emb);
            }
        }

        if seed_embeddings.is_empty() {
            return Ok(Vec::new());
        }

        // Compute centroid (average) of all seed embeddings
        let embedding_dim = seed_embeddings[0].len();
        let mut centroid = vec![0.0f32; embedding_dim];
        for emb in &seed_embeddings {
            for (i, &val) in emb.iter().enumerate() {
                centroid[i] += val;
            }
        }
        for val in &mut centroid {
            *val /= seed_embeddings.len() as f32;
        }

        // Normalize the centroid for L2 distance
        let centroid = Self::normalize_embedding(centroid);
        let vec_str = format!(
            "[{}]",
            centroid
                .iter()
                .map(|v| v.to_string())
                .collect::<Vec<_>>()
                .join(",")
        );

        // Combine exclude_ids with seed_ids for exclusion
        let mut all_exclude: Vec<String> = exclude_ids.to_vec();
        all_exclude.extend(seed_ids.iter().cloned());

        // Find tracks closest to the centroid that share genres with seeds
        // Strategy: Collect ALL genres from ALL seed tracks, then only include tracks
        // that have at least one genre matching that combined set
        let results = sqlx::query_as::<_, (String, f64)>(
            r#"
            WITH seed_genres AS (
                -- Collect all unique genres from all seed tracks
                SELECT DISTINCT g.genre
                FROM library_index li,
                     jsonb_array_elements_text(li.genres) AS g(genre)
                WHERE li.id = ANY($4)
            ),
            allowed_genres AS (
                SELECT array_agg(genre) as genres FROM seed_genres
            )
            SELECT
                te.track_id,
                1.0 - (te.embedding <-> $1::vector) / 2.0 as similarity
            FROM track_embeddings te
            JOIN library_index li ON te.track_id = li.id
            CROSS JOIN allowed_genres ag
            WHERE te.track_id != ALL($2)
            AND li.genres ?| ag.genres  -- Track has at least one genre from the seed genres
            ORDER BY te.embedding <-> $1::vector
            LIMIT $3
            "#,
        )
        .bind(&vec_str)
        .bind(&all_exclude)
        .bind(limit as i64)
        .bind(seed_ids)
        .fetch_all(&self.db)
        .await?;

        Ok(results
            .into_iter()
            .map(|(id, sim)| (id, sim as f32))
            .collect())
    }

    // ========================================
    // Visualization Cache Functions
    // ========================================

    /// Check if visualization cache needs to be rebuilt
    /// Returns true if cache is stale (embedding count changed or no cache exists)
    pub async fn is_visualization_cache_stale(&self) -> Result<bool> {
        // Get current embedding count
        let current_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*)::int8 FROM track_embeddings"
        )
        .fetch_one(&self.db)
        .await?;

        // Get cached count from visualization_config
        let cached_count: Option<i32> = sqlx::query_scalar(
            "SELECT track_count FROM visualization_config WHERE id = 1"
        )
        .fetch_optional(&self.db)
        .await?;

        match cached_count {
            Some(count) if count as i64 == current_count => {
                // Check if any embeddings are missing viz coordinates
                let missing: i64 = sqlx::query_scalar(
                    "SELECT COUNT(*)::int8 FROM track_embeddings WHERE viz_x IS NULL"
                )
                .fetch_one(&self.db)
                .await?;
                Ok(missing > 0)
            }
            _ => Ok(true), // No cache or count mismatch
        }
    }

    /// Compute neighborhood-preserving embedding and update visualization cache
    /// Uses PCA initialization followed by force-directed refinement to preserve local structure
    pub async fn rebuild_visualization_cache(&self) -> Result<()> {
        tracing::info!("Rebuilding visualization cache...");

        // Fetch all embeddings
        let rows: Vec<(String, Vec<f32>)> = sqlx::query_as(
            "SELECT track_id, embedding::real[] FROM track_embeddings ORDER BY track_id"
        )
        .fetch_all(&self.db)
        .await?;

        if rows.is_empty() {
            tracing::info!("No embeddings to visualize");
            return Ok(());
        }

        let n_samples = rows.len();
        let n_features = rows[0].1.len();
        tracing::info!("Computing neighborhood embedding for {} embeddings with {} features", n_samples, n_features);

        // Collect embeddings into a matrix
        let embeddings: Vec<Vec<f32>> = rows.iter().map(|(_, emb)| emb.clone()).collect();
        let track_ids: Vec<String> = rows.iter().map(|(id, _)| id.clone()).collect();

        // Compute mean for centering
        let mut mean = vec![0.0f32; n_features];
        for emb in &embeddings {
            for (i, &v) in emb.iter().enumerate() {
                mean[i] += v;
            }
        }
        for v in &mut mean {
            *v /= n_samples as f32;
        }

        // Center the data
        let centered: Vec<Vec<f32>> = embeddings
            .iter()
            .map(|emb| emb.iter().zip(&mean).map(|(e, m)| e - m).collect())
            .collect();

        // Step 1: PCA for initial layout
        let (pc1, pc2) = Self::power_iteration_pca(&centered, n_features);
        let mut positions: Vec<(f32, f32)> = centered
            .iter()
            .map(|c| {
                let x: f32 = c.iter().zip(&pc1).map(|(a, b)| a * b).sum();
                let y: f32 = c.iter().zip(&pc2).map(|(a, b)| a * b).sum();
                (x, y)
            })
            .collect();

        // Step 2: Compute k-nearest neighbors in high-dimensional space
        // This is used to apply attractive forces between neighbors
        let k = 15.min(n_samples - 1); // Number of neighbors
        tracing::info!("Computing {} nearest neighbors...", k);
        let neighbors = Self::compute_knn(&embeddings, k);

        // Step 3: Force-directed refinement (simplified t-SNE-like optimization)
        // This adjusts positions to pull neighbors closer together
        tracing::info!("Refining layout with force-directed optimization...");
        let iterations = 100;
        let learning_rate = 0.5f32;

        for iter in 0..iterations {
            let mut forces: Vec<(f32, f32)> = vec![(0.0, 0.0); n_samples];

            // Attractive forces between neighbors
            for i in 0..n_samples {
                for &j in &neighbors[i] {
                    let (xi, yi) = positions[i];
                    let (xj, yj) = positions[j];
                    let dx = xj - xi;
                    let dy = yj - yi;
                    let dist = (dx * dx + dy * dy).sqrt().max(0.01);

                    // Attractive force (pull neighbors together)
                    let attraction = 0.1 * dist;
                    forces[i].0 += attraction * dx / dist;
                    forces[i].1 += attraction * dy / dist;
                }
            }

            // Repulsive forces between all points (approximated with random sampling for speed)
            // Sample a subset of points for repulsion to keep O(n) complexity
            let repulsion_samples = 50.min(n_samples);
            for i in 0..n_samples {
                for s in 0..repulsion_samples {
                    let j = (i + s * 7 + iter * 13) % n_samples; // Pseudo-random sampling
                    if i == j { continue; }

                    let (xi, yi) = positions[i];
                    let (xj, yj) = positions[j];
                    let dx = xi - xj;
                    let dy = yi - yj;
                    let dist_sq = (dx * dx + dy * dy).max(0.0001);

                    // Repulsive force (push non-neighbors apart)
                    let repulsion = 0.001 / dist_sq;
                    forces[i].0 += repulsion * dx;
                    forces[i].1 += repulsion * dy;
                }
            }

            // Apply forces with momentum decay
            let lr = learning_rate * (1.0 - iter as f32 / iterations as f32);
            for i in 0..n_samples {
                positions[i].0 += forces[i].0 * lr;
                positions[i].1 += forces[i].1 * lr;
            }
        }

        // Normalize to [-1, 1] range for consistent visualization
        let (min_x, max_x, min_y, max_y) = positions.iter().fold(
            (f32::MAX, f32::MIN, f32::MAX, f32::MIN),
            |(min_x, max_x, min_y, max_y), (x, y)| {
                (min_x.min(*x), max_x.max(*x), min_y.min(*y), max_y.max(*y))
            },
        );
        let range_x = (max_x - min_x).max(1e-6);
        let range_y = (max_y - min_y).max(1e-6);

        // Update database in a transaction
        let mut tx = self.db.begin().await?;

        // Store PCA config for consistent future projections
        sqlx::query(
            r#"
            INSERT INTO visualization_config (id, pc1, pc2, mean_vec, track_count)
            VALUES (1, $1, $2, $3, $4)
            ON CONFLICT (id) DO UPDATE SET
                pc1 = EXCLUDED.pc1,
                pc2 = EXCLUDED.pc2,
                mean_vec = EXCLUDED.mean_vec,
                track_count = EXCLUDED.track_count,
                updated_at = NOW()
            "#
        )
        .bind(&pc1)
        .bind(&pc2)
        .bind(&mean)
        .bind(n_samples as i32)
        .execute(&mut *tx)
        .await?;

        // Update viz coordinates for all tracks
        for (i, track_id) in track_ids.iter().enumerate() {
            let (x, y) = positions[i];
            let norm_x = 2.0 * (x - min_x) / range_x - 1.0;
            let norm_y = 2.0 * (y - min_y) / range_y - 1.0;

            sqlx::query(
                "UPDATE track_embeddings SET viz_x = $1, viz_y = $2 WHERE track_id = $3"
            )
            .bind(norm_x)
            .bind(norm_y)
            .bind(track_id)
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        tracing::info!("Visualization cache rebuilt for {} tracks", n_samples);
        Ok(())
    }

    /// Compute k-nearest neighbors for each point using memory-efficient approach
    /// Uses a max-heap of size k instead of storing all n-1 distances
    /// Returns a vector of neighbor indices for each point
    fn compute_knn(embeddings: &[Vec<f32>], k: usize) -> Vec<Vec<usize>> {
        use std::collections::BinaryHeap;
        use std::cmp::Ordering;

        // Wrapper for max-heap (we want k smallest, so invert comparison)
        #[derive(PartialEq)]
        struct MaxDist(f32, usize);

        impl Eq for MaxDist {}

        impl PartialOrd for MaxDist {
            fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
                // Reverse order for max-heap behavior
                self.0.partial_cmp(&other.0)
            }
        }

        impl Ord for MaxDist {
            fn cmp(&self, other: &Self) -> Ordering {
                self.partial_cmp(other).unwrap_or(Ordering::Equal)
            }
        }

        let n = embeddings.len();
        let mut neighbors = Vec::with_capacity(n);

        // Log progress every 10%
        let log_interval = (n / 10).max(1);

        for i in 0..n {
            if i % log_interval == 0 && i > 0 {
                tracing::debug!("KNN progress: {}/{} ({:.0}%)", i, n, (i as f32 / n as f32) * 100.0);
            }

            // Use a max-heap of size k to track k smallest distances
            let mut heap: BinaryHeap<MaxDist> = BinaryHeap::with_capacity(k + 1);

            for j in 0..n {
                if i == j { continue; }

                let dist: f32 = embeddings[i]
                    .iter()
                    .zip(&embeddings[j])
                    .map(|(a, b)| (a - b).powi(2))
                    .sum();

                if heap.len() < k {
                    heap.push(MaxDist(dist, j));
                } else if let Some(max) = heap.peek() {
                    if dist < max.0 {
                        heap.pop();
                        heap.push(MaxDist(dist, j));
                    }
                }
            }

            // Extract neighbor indices (already the k nearest)
            let mut knn: Vec<usize> = heap.into_iter().map(|MaxDist(_, j)| j).collect();
            knn.sort(); // Optional: sort by index for consistency
            neighbors.push(knn);
        }

        neighbors
    }

    /// Power iteration to find top 2 principal components
    /// More efficient than full SVD for our use case
    fn power_iteration_pca(centered: &[Vec<f32>], n_features: usize) -> (Vec<f32>, Vec<f32>) {
        let n_samples = centered.len();

        // Initialize PC1 with a fixed seed for consistency
        let mut pc1: Vec<f32> = (0..n_features).map(|i| ((i * 7 + 11) % 100) as f32 / 100.0).collect();
        Self::normalize_vec(&mut pc1);

        // Power iteration for PC1 (20 iterations is usually enough)
        for _ in 0..20 {
            // Multiply by X^T * X
            let mut new_pc1 = vec![0.0f32; n_features];
            for row in centered {
                let dot: f32 = row.iter().zip(&pc1).map(|(a, b)| a * b).sum();
                for (i, &v) in row.iter().enumerate() {
                    new_pc1[i] += v * dot;
                }
            }
            pc1 = new_pc1;
            Self::normalize_vec(&mut pc1);
        }

        // Deflate: subtract PC1 projection from data
        let mut deflated = centered.to_vec();
        for row in &mut deflated {
            let dot: f32 = row.iter().zip(&pc1).map(|(a, b)| a * b).sum();
            for (i, v) in row.iter_mut().enumerate() {
                *v -= dot * pc1[i];
            }
        }

        // Initialize PC2 with different seed
        let mut pc2: Vec<f32> = (0..n_features).map(|i| ((i * 13 + 17) % 100) as f32 / 100.0).collect();
        Self::normalize_vec(&mut pc2);

        // Power iteration for PC2
        for _ in 0..20 {
            let mut new_pc2 = vec![0.0f32; n_features];
            for row in &deflated {
                let dot: f32 = row.iter().zip(&pc2).map(|(a, b)| a * b).sum();
                for (i, &v) in row.iter().enumerate() {
                    new_pc2[i] += v * dot;
                }
            }
            pc2 = new_pc2;
            Self::normalize_vec(&mut pc2);
        }

        (pc1, pc2)
    }

    fn normalize_vec(v: &mut Vec<f32>) {
        let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 1e-10 {
            for x in v.iter_mut() {
                *x /= norm;
            }
        }
    }

    /// Project a single new embedding using the cached PCA config
    /// Used when adding new embeddings to maintain consistent visualization
    pub async fn project_single_embedding(&self, track_id: &str, embedding: &[f32]) -> Result<()> {
        // Get PCA config
        let config: Option<(Vec<f32>, Vec<f32>, Vec<f32>)> = sqlx::query_as(
            "SELECT pc1, pc2, mean_vec FROM visualization_config WHERE id = 1"
        )
        .fetch_optional(&self.db)
        .await?;

        let (pc1, pc2, mean) = match config {
            Some(c) => c,
            None => {
                // No PCA config yet, will be computed on next cache rebuild
                return Ok(());
            }
        };

        // Center the embedding
        let centered: Vec<f32> = embedding.iter().zip(&mean).map(|(e, m)| e - m).collect();

        // Project onto PCs
        let x: f32 = centered.iter().zip(&pc1).map(|(a, b)| a * b).sum();
        let y: f32 = centered.iter().zip(&pc2).map(|(a, b)| a * b).sum();

        // Store (unnormalized for now - will be renormalized on full rebuild)
        sqlx::query(
            "UPDATE track_embeddings SET viz_x = $1, viz_y = $2 WHERE track_id = $3"
        )
        .bind(x)
        .bind(y)
        .bind(track_id)
        .execute(&self.db)
        .await?;

        Ok(())
    }

    /// Get embedding processing status
    pub async fn get_status(&self) -> Result<EmbeddingStatus> {
        // Get basic counts
        let total_tracks = sqlx::query_scalar!(
            "SELECT COUNT(*)::int4 as \"count!\" FROM library_index"
        )
        .fetch_one(&self.db)
        .await?;

        let with_embeddings = sqlx::query_scalar!(
            "SELECT COUNT(*)::int4 as \"count!\" FROM track_embeddings"
        )
        .fetch_one(&self.db)
        .await?;

        let failed = sqlx::query_scalar!(
            "SELECT COUNT(*)::int4 as \"count!\" FROM embedding_failures"
        )
        .fetch_one(&self.db)
        .await?;

        let pending = total_tracks - with_embeddings - failed;
        let coverage = if total_tracks > 0 {
            (with_embeddings as f64 / total_tracks as f64) * 100.0
        } else {
            0.0
        };

        Ok(EmbeddingStatus {
            total_tracks,
            tracks_with_embeddings: with_embeddings,
            tracks_pending: pending,
            tracks_failed: failed,
            coverage_percent: coverage,
            avg_processing_time_ms: None,
            model_version: "v1".to_string(),
            updated_at: chrono::Utc::now(),
        })
    }
}

#[derive(Debug, Clone)]
pub struct EmbeddingStatus {
    pub total_tracks: i32,
    pub tracks_with_embeddings: i32,
    pub tracks_pending: i32,
    pub tracks_failed: i32,
    pub coverage_percent: f64,
    pub avg_processing_time_ms: Option<f64>,
    pub model_version: String,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}
