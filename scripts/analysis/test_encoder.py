#!/usr/bin/env python3
"""
Test the audio encoder with real audio files to check if embeddings differ across genres.
"""

import numpy as np
import torch
import librosa
from pathlib import Path

# Load the same model we exported
from export_audio_encoder import AudioEncoder
from huggingface_hub import hf_hub_download


def load_model():
    """Load the audio encoder model with trained weights."""
    model = AudioEncoder()
    weights_path = hf_hub_download('teticio/audio-encoder', 'diffusion_pytorch_model.bin')
    state_dict = torch.load(weights_path, map_location='cpu', weights_only=True)
    model.load_state_dict(state_dict)
    model.eval()
    return model


def preprocess_audio(audio_path: str, sr: int = 22050, n_mels: int = 96,
                      n_fft: int = 2048, hop_length: int = 512,
                      target_frames: int = 216, top_db: float = 80.0):
    """
    Preprocess audio file to mel spectrogram.

    Matches the audiodiffusion preprocessing:
    1. Load audio at 22050 Hz
    2. Compute mel spectrogram
    3. Convert to dB (power_to_db with ref=max, top_db=80)
    4. Normalize to [0, 1]
    5. Resize to target_frames
    """
    # Load audio
    y, sr = librosa.load(audio_path, sr=sr, mono=True)

    # Compute mel spectrogram
    mel_spec = librosa.feature.melspectrogram(
        y=y, sr=sr, n_fft=n_fft, hop_length=hop_length, n_mels=n_mels
    )

    # Convert to dB with ref=max, top_db=80
    mel_spec_db = librosa.power_to_db(mel_spec, ref=np.max, top_db=top_db)

    # Normalize to [0, 1]: (S + top_db) / top_db
    mel_spec_norm = (mel_spec_db + top_db) / top_db

    # Resize to target frames
    from scipy.ndimage import zoom
    current_frames = mel_spec_norm.shape[1]
    if current_frames != target_frames:
        scale = target_frames / current_frames
        mel_spec_norm = zoom(mel_spec_norm, (1, scale), order=1)

    # Ensure exact size
    if mel_spec_norm.shape[1] > target_frames:
        mel_spec_norm = mel_spec_norm[:, :target_frames]
    elif mel_spec_norm.shape[1] < target_frames:
        pad = target_frames - mel_spec_norm.shape[1]
        mel_spec_norm = np.pad(mel_spec_norm, ((0, 0), (0, pad)), mode='constant')

    # Convert to tensor with batch and channel dimensions
    tensor = torch.from_numpy(mel_spec_norm.astype(np.float32))
    tensor = tensor.unsqueeze(0).unsqueeze(0)  # (1, 1, n_mels, target_frames)

    return tensor


def cosine_similarity(a, b):
    """Compute cosine similarity between two vectors."""
    return np.dot(a, b) / (np.linalg.norm(a) * np.linalg.norm(b))


def main():
    print("Loading model...")
    model = load_model()

    # Test files (from the library)
    library_path = Path("/Volumes/tank/navidrome/music")

    # Find some audio files
    audio_files = []
    for ext in ['*.mp3', '*.flac', '*.m4a']:
        audio_files.extend(list(library_path.rglob(ext))[:2])
        if len(audio_files) >= 6:
            break

    if len(audio_files) < 2:
        print("Not enough audio files found!")
        return

    print(f"\nFound {len(audio_files)} audio files")

    # Generate embeddings
    embeddings = []
    for audio_path in audio_files[:6]:
        print(f"\nProcessing: {audio_path.name}")
        try:
            mel_tensor = preprocess_audio(str(audio_path))
            print(f"  Mel shape: {mel_tensor.shape}")
            print(f"  Mel stats: min={mel_tensor.min():.4f}, max={mel_tensor.max():.4f}, mean={mel_tensor.mean():.4f}")

            with torch.no_grad():
                embedding = model(mel_tensor).numpy().flatten()

            print(f"  Embedding shape: {embedding.shape}")
            print(f"  Embedding stats: min={embedding.min():.4f}, max={embedding.max():.4f}, mean={embedding.mean():.4f}")
            print(f"  First 5 values: {embedding[:5]}")

            embeddings.append((audio_path.name, embedding))
        except Exception as e:
            print(f"  Error: {e}")

    # Compute pairwise similarities
    print("\n" + "="*60)
    print("Pairwise Cosine Similarities:")
    print("="*60)

    for i in range(len(embeddings)):
        for j in range(i+1, len(embeddings)):
            name1, emb1 = embeddings[i]
            name2, emb2 = embeddings[j]
            sim = cosine_similarity(emb1, emb2)
            print(f"{name1[:30]:30} vs {name2[:30]:30}: {sim:.4f}")


if __name__ == "__main__":
    main()
