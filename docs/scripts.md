# Development Scripts

This directory contains Python scripts for working with the audio embedding system.

## Prerequisites

```bash
# Create virtual environment
python3 -m venv venv
source venv/bin/activate

# Install dependencies
pip install torch numpy scikit-learn psycopg2-binary matplotlib
```

## Model Scripts (`scripts/model/`)

### `export_audio_encoder.py`
Exports the teticio/audio-encoder model from HuggingFace to ONNX format.

```bash
python scripts/model/export_audio_encoder.py
```

This creates `audio_encoder.onnx` which is used by the Rust backend for generating audio embeddings. The model:
- Input: Mel spectrogram (1, 96, 216) - 96 mel bins, 216 time frames
- Output: 100-dimensional embedding vector
- Trained on 1M+ Spotify playlists for music similarity

### `convert_to_onnx.py`
Alternative ONNX conversion script with additional validation.

## Analysis Scripts (`scripts/analysis/`)

These scripts analyze the quality of audio embeddings stored in the database.

### `analyze_clusters.py`
Clusters tracks by their embeddings and analyzes if clusters correspond to musical similarity.

```bash
python scripts/analysis/analyze_clusters.py
```

### `analyze_playlist_similarity.py`
Compares tracks within playlists/stations to verify that similar-sounding tracks have similar embeddings.

```bash
python scripts/analysis/analyze_playlist_similarity.py
```

### `analyze_station_similarity.py`
Analyzes embedding similarity within radio stations.

```bash
python scripts/analysis/analyze_station_similarity.py
```

### `analyze_relative_similarity.py`
Computes relative similarity metrics across the embedding space.

```bash
python scripts/analysis/analyze_relative_similarity.py
```

### `test_encoder.py` / `test_encoder_synthetic.py`
Unit tests for the audio encoder preprocessing pipeline.

```bash
python scripts/analysis/test_encoder.py
python scripts/analysis/test_encoder_synthetic.py
```

### `test_rust_preprocess.py`
Validates that Rust preprocessing matches Python preprocessing output.

```bash
python scripts/analysis/test_rust_preprocess.py
```

## Database Connection

Analysis scripts connect to PostgreSQL with these defaults:
- Host: `localhost:5432`
- Database: `navidrome_radio`
- User: `postgres`
- Password: `postgres`

Modify the connection parameters in each script if needed.
